import { api, type Cell, type ColumnMeta, type Provenance, type RowKey } from "../api";
import { cellText } from "./format";

/** Rust's default `rowLimit`, for the moment before settings load; Rust clamps the page size anyway. */
export const DEFAULT_ROW_LIMIT = 1000;

/** MySQL column names are case-insensitive, so a result header may differ in case from the schema. */
export function columnIndex(columns: ColumnMeta[], name: string): number {
  const wanted = name.toLowerCase();
  return columns.findIndex((c) => c.name.toLowerCase() === wanted);
}

/** A row key as the `keyColumn` and `key` that row changes address; null without a key or for a composite one. */
export function singleKey(key: RowKey | null | undefined): { keyColumn: string; key: string } | null {
  return key?.length === 1 ? { keyColumn: key[0].column, key: key[0].value } : null;
}

/** Where a grid's rows came from, so `longText` cells can be fetched in full. */
export interface CellSource {
  sessionId: string;
  resultId: number;
}

/** The full text of a cell; a `longText` cell comes from the result set Rust keeps. */
export async function fullCellText(cell: Cell, source: CellSource, row: number, col: number): Promise<string> {
  if (cell.t !== "longText") return cellText(cell);
  return api.fetchCell(source.sessionId, source.resultId, row, col);
}

/** The value at `source` as stored now, read by its row key (so row order does not matter); null when no row has that key any more. */
export async function fetchStoredValue(sessionId: string, database: string, source: Provenance): Promise<{ cell: Cell; text: string } | null> {
  const where = source.row.map((part) => ({ column: part.column, value: part.value }));
  const page = await api.browseTable(sessionId, { database, table: source.table, offset: 0, limit: 1, where });
  if (page.result.rows.length === 0) return null;
  const col = columnIndex(page.result.columns, source.column);
  if (col < 0) throw new Error(`${source.table} has no column ${source.column}`);
  const cell = page.result.rows[0][col];
  return { cell, text: await fullCellText(cell, { sessionId, resultId: page.result.id }, 0, col) };
}
