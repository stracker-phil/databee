import type { Cell, CheckedRow } from "../api";

/** How often the active table tab checks the rows it shows. */
export const TABLE_CHECK_MS = 4000;

/** A shown row the database no longer holds as shown. */
export interface RowChange {
  kind: "changed" | "removed";
  /** When the check first saw it. */
  at: number;
  /** The checksum `cells` were read at; null until the row was read. */
  hash: string | null;
  /** The row as stored, in page column order; null until read (and for a removed row). */
  cells: Cell[] | null;
  /** Columns whose stored value differs from the shown one; empty when only a hidden part changed. */
  columns: number[];
}

export interface TableCheckState {
  /** The page's result set; a new page starts over. */
  resultId: number;
  /** Checksums of the page's rows as the first check read them; null until it answered. */
  baseline: (string | null)[] | null;
  /** Changed and removed rows by row index. */
  rows: Record<number, RowChange>;
  /** Why checking stopped for this page. */
  error: string | null;
}

export const freshCheck = (resultId: number): TableCheckState => ({ resultId, baseline: null, rows: {}, error: null });

/**
 * One check's checksums against the baseline: a row with its baseline checksum is unmarked, a row
 * with another one is changed, a row without one is removed. The first check is the baseline;
 * a row already gone then is removed.
 */
export function withHashes(state: TableCheckState, hashes: (string | null)[], at: number): TableCheckState {
  const baseline = state.baseline ?? hashes;
  const rows: Record<number, RowChange> = {};
  hashes.forEach((hash, row) => {
    if (hash !== null && hash === baseline[row]) return;
    const kind = hash === null ? "removed" : "changed";
    const before = state.rows[row];
    if (before?.kind === kind) rows[row] = before;
    else rows[row] = { kind, at, hash: null, cells: null, columns: [] };
  });
  return { ...state, baseline, rows };
}

/** Changed rows whose stored values are not read at their current checksum yet. */
export function rowsToRead(state: TableCheckState, hashes: (string | null)[]): number[] {
  return Object.entries(state.rows)
    .filter(([row, change]) => change.kind === "changed" && change.hash !== hashes[Number(row)])
    .map(([row]) => Number(row));
}

/** The stored values of rows that are still changed; a row read back at its baseline is unmarked. */
export function withRows(state: TableCheckState, checked: CheckedRow[]): TableCheckState {
  const rows = { ...state.rows };
  for (const { row, hash, cells, changed } of checked) {
    const change = rows[row];
    if (change?.kind !== "changed") continue;
    if (hash === state.baseline?.[row]) delete rows[row];
    else rows[row] = { ...change, hash, cells, columns: changed };
  }
  return { ...state, rows };
}

export function changeCounts(rows: TableCheckState["rows"] | undefined): { changed: number; removed: number } {
  const changes = Object.values(rows ?? {});
  return { changed: changes.filter((c) => c.kind === "changed").length, removed: changes.filter((c) => c.kind === "removed").length };
}

const rowsText = (n: number) => (n === 1 ? "1 row" : `${n} rows`);

/** `3 rows changed in the database`, `2 rows changed and 1 row removed in the database`; null when nothing did. */
export function changesText({ changed, removed }: { changed: number; removed: number }): string | null {
  if (changed > 0 && removed > 0) return `${rowsText(changed)} changed and ${rowsText(removed)} removed in the database`;
  if (changed > 0) return `${rowsText(changed)} changed in the database`;
  if (removed > 0) return `${rowsText(removed)} removed from the database`;
  return null;
}
