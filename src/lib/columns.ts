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
  /** The `dataVersion` the text was read at; once the data changes, the column reads its value again. */
  version?: number;
  /** The plans this column's edits queued; while one is pending, a reload keeps the edited text. */
  queued?: number[];
  /** The value's row was gone when the column read it again. */
  missing?: boolean;
}

export type StackColumn = ObjectColumn | ValueColumn;

export interface ColumnStack {
  columns: StackColumn[];
  active: number;
}

/** The narrowest a pane gets while others show next to it. */
export const MIN_PANE_WIDTH = 560;
/** Before the settings load; Rust's `Settings::default` is the source of truth. */
export const DEFAULT_MAX_PANES = 2;
/** The hairline divider between two panes. */
export const DIVIDER_WIDTH = 1;
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
 * The columns shown as panes, `[start, end)`: at most `maxPanes` and as many as fit at `MIN_PANE_WIDTH` (one
 * always), ending at the newest unless the active column lies further left.
 */
export function shownColumns(count: number, active: number, width: number, maxPanes: number): { start: number; end: number } {
  const fit = Math.floor((width + DIVIDER_WIDTH) / (MIN_PANE_WIDTH + DIVIDER_WIDTH));
  const shown = Math.max(1, Math.min(count, maxPanes, fit));
  const start = Math.max(0, Math.min(active, count - shown));
  return { start, end: start + shown };
}

/** A pane can be maximized next to others (several columns, several panes allowed); a maximized one can always be restored. */
export function maximizable(stack: { columns: StackColumn[]; maximized?: boolean }, maxPanes: number): boolean {
  return !!stack.maximized || (stack.columns.length > 1 && maxPanes > 1);
}

/** The share of each of `count` panes: `widths` when it has one per pane, else equal shares. */
export function paneShares(widths: number[] | undefined, count: number): number[] {
  return widths?.length === count ? widths : Array<number>(count).fill(1 / count);
}
