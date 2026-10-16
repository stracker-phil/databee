import { api, type Cell } from "../api";
import { cellText } from "./format";

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
