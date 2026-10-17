import { api, type Cell, type ColumnMeta, type RowKey } from "../api";
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
