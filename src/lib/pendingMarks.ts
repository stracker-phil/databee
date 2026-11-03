import type { ColumnMeta, PlannedChange, RowKey } from "../api";
import { columnIndex, singleKey } from "./cells";

/** How pending changes touch one grid row. */
export interface RowMark {
  deleted: boolean;
  duplicated: boolean;
  /** Indexes of the columns a pending edit sets. */
  columns: Set<number>;
  /** The descriptions of the plans touching the row, one per line, for a tooltip. */
  title: string;
}

interface Grid {
  sessionId: string;
  database: string;
  table: string;
  keys: readonly (RowKey | null | undefined)[];
  columns: ColumnMeta[];
}

const rowId = (keyColumn: string, key: string) => `${keyColumn.toLowerCase()}\u0000${key}`;

type Pending = readonly (PlannedChange & { sessionId: string })[];

function collectRowMarks(pending: Pending, grid: Grid): Map<string, RowMark> {
  const byRow = new Map<string, RowMark>();
  for (const change of pending) {
    if (change.sessionId !== grid.sessionId) continue;
    for (const row of change.rows.filter((r) => r.database === grid.database && r.table === grid.table)) {
      const id = rowId(row.keyColumn, row.key);
      const mark = byRow.get(id) ?? { deleted: false, duplicated: false, columns: new Set<number>(), title: "" };
      if (row.effect.kind === "delete") mark.deleted = true;
      if (row.effect.kind === "duplicate") mark.duplicated = true;
      const col = row.effect.kind === "set" ? columnIndex(grid.columns, row.effect.column) : -1;
      if (col >= 0) mark.columns.add(col);
      mark.title = mark.title ? `${mark.title}\n${change.description}` : change.description;
      byRow.set(id, mark);
    }
  }
  return byRow;
}

/** The marks of a table grid's rows, by row index, from the plans waiting for review. */
export function pendingRowMarks(pending: Pending, grid: Grid): Record<number, RowMark> {
  const byRow = collectRowMarks(pending, grid);
  const marks: Record<number, RowMark> = {};
  if (byRow.size === 0) return marks;
  grid.keys.forEach((key, index) => {
    const single = singleKey(key);
    if (!single) return;
    const mark = byRow.get(rowId(single.keyColumn, single.key));
    if (mark) marks[index] = mark;
  });
  return marks;
}
