/** The trail above an object tab's panes; crumbs of hidden columns fold into "…". */

/** A "…" item: the folded columns (and the table, once everything else is folded) it lists in its menu. */
export interface TrailMore {
  kind: "more";
  side: "before" | "after";
  table: boolean;
  columns: number[];
}

export type TrailUnit = { kind: "table" } | { kind: "column"; index: number } | TrailMore;

const range = (from: number, to: number) => Array.from({ length: Math.max(0, to - from) }, (_, i) => from + i);

/**
 * The columns in the order they fold: those between the root row and the panes (the oldest first), then the
 * root row, then those right of the panes (the newest first). Shown columns never fold; the table folds last.
 */
function foldOrder(count: number, start: number, end: number): number[] {
  const before = start > 0 ? [...range(1, start), 0] : [];
  return [...before, ...range(end, count).reverse()];
}

/** Counts the table's step when there is one. */
export const foldSteps = (count: number, start: number, end: number, table: boolean) => foldOrder(count, start, end).length + (table ? 1 : 0);

const more = (side: TrailMore["side"], columns: number[], table = false): TrailMore => ({ kind: "more", side, table, columns: columns.sort((a, b) => a - b) });

/** The trail's items after the first `steps` folds. */
export function trailUnits(count: number, start: number, end: number, table: boolean, steps: number): TrailUnit[] {
  const order = foldOrder(count, start, end);
  const folded = order.slice(0, steps);
  const tableFolded = table && steps > order.length;
  const before = more("before", folded.filter((i) => i < start), tableFolded);
  const after = more("after", folded.filter((i) => i >= end));
  const units: TrailUnit[] = table && !tableFolded ? [{ kind: "table" }] : [];
  if (tableFolded && before.columns.length === 0) units.push(before);
  for (let index = 0; index < count; index++) {
    if (before.columns.includes(index)) {
      if (index === before.columns[0]) units.push(before);
      continue;
    }
    if (after.columns.includes(index)) {
      if (index === after.columns[0]) units.push(after);
      continue;
    }
    units.push({ kind: "column", index });
  }
  return units;
}
