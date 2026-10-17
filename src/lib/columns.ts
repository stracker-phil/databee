import type { Address, Provenance, Value } from "../api";
import { cellText } from "./format";
import { objectAddress, type ObjectView } from "./document";

export interface ObjectColumn {
  kind: "object";
  /** A row address; `address.view` is null for the document and "rows" for rows. */
  address: Address;
  view: ObjectView;
  /** The focused value, kept across the Document/Rows toggle. */
  focus: Provenance | null;
}

export interface ValueColumn {
  kind: "value";
  value: Value;
  /** The full stored text; after an edit is queued, the edited text. */
  text: string;
  database: string | null;
}

export type StackColumn = ObjectColumn | ValueColumn;

export interface ColumnStack {
  columns: StackColumn[];
  active: number;
}

export const MIN_COLUMN_WIDTH = 420;
export const STRIP_WIDTH = 34;
/** How much of a value without provenance names its column. */
const UNSOURCED_NAME_CHARS = 20;

/** An object column of a row address; a document keeps the address's `compare`. */
export function objectColumn(address: Address): ObjectColumn {
  const view: ObjectView = address.view === "rows" ? "rows" : "document";
  const compare = view === "document" ? address.compare : null;
  return { kind: "object", address: { ...objectAddress(address, view), compare }, view, focus: null };
}

/** The object column at `index`, or the nearest one to its left: a value column belongs to the row it came from. */
export function objectColumnAt(columns: StackColumn[], index: number): ObjectColumn | null {
  for (let i = Math.min(index, columns.length - 1); i >= 0; i--) {
    const column = columns[i];
    if (column.kind === "object") return column;
  }
  return null;
}

/** The table and key of an object column (`wp_users`, `12`), or a value's column name and no key. */
export function columnName(column: StackColumn): { name: string; key: string | null } {
  if (column.kind === "value") return { name: column.value.source?.column ?? cellText(column.value.raw).slice(0, UNSOURCED_NAME_CHARS), key: null };
  const target = column.address.target;
  return target.kind === "row" ? { name: target.table, key: target.key } : { name: target.kind, key: null };
}

export function columnLabel(column: StackColumn): string {
  const { name, key } = columnName(column);
  return key === null ? name : `${name} #${key}`;
}

/**
 * The columns shown in full, `[start, end)`: as many as fit at `MIN_COLUMN_WIDTH` next to the strips of the
 * others, ending at the newest unless the active column lies further left.
 */
export function shownColumns(count: number, active: number, width: number): { start: number; end: number } {
  const fit = Math.floor((width - count * STRIP_WIDTH) / (MIN_COLUMN_WIDTH - STRIP_WIDTH));
  const shown = Math.max(1, Math.min(count, fit));
  const start = Math.max(0, Math.min(active, count - shown));
  return { start, end: start + shown };
}
