/**
 * A cell as a `Value` (kind and provenance), built from the result's column kinds and serialized
 * cells instead of one `Value` per cell from Rust (payload size). A port of Rust's
 * `value::value_at`, the reference implementation; tests keep both equal.
 */
import type { Address, BrowsePage, Cell, ColumnKind, Provenance, ReferenceTarget, ResultSet, SerializedCell, Target, Value, ValueKind } from "../api";
import { address } from "./address";

/** The value of one cell of a browse page, sourced from the page's table and the row's key. */
export function valueAt(page: BrowsePage, row: number, col: number): Value {
  const key = page.keys[row];
  const source: Provenance | null = key ? { table: page.table, column: page.result.columns[col].name, row: key } : null;
  return resultValueAt(page.result, row, col, source);
}

/** The value of one cell of any result; query results have no provenance yet. */
export function resultValueAt(result: ResultSet, row: number, col: number, source: Provenance | null = null): Value {
  const raw = result.rows[row][col];
  return { raw, kind: cellKind(result, row, col, raw), source, style: null, secondary: null };
}

const cellText = (cell: Cell): string | null => (cell.t === "number" || cell.t === "text" || cell.t === "temporal" || cell.t === "json" ? cell.v : null);

/** WordPress stores "no target" as `0`; NULL means the same in a reference column. */
const isNoReference = (cell: Cell) => cell.t === "null" || cellText(cell) === "0";

function cellKind(result: ResultSet, row: number, col: number, cell: Cell): ValueKind {
  const column: ColumnKind = result.kinds[col] ?? { kind: "text" };
  if (column.kind === "reference" && isNoReference(cell)) return { kind: "noReference" };
  if (cell.t === "null") return { kind: "null" };
  if (cell.t === "text" && cell.v === "") return { kind: "empty" };
  if (cell.t === "bytes") return { kind: "binary" };

  const fromSchema = schemaKind(result, row, column, cell);
  if (fromSchema) return fromSchema;
  const serialized = findCell(result.serialized, row, col);
  if (serialized) return { kind: "serialized", format: serialized.format, preview: serialized.preview };
  if (column.kind === "number") return { kind: "number" };
  if (column.kind === "timestamp") return { kind: "timestamp", utc: null };
  return { kind: "text" };
}

/** `primaryKey` or `reference`, when the cell's text can address a row. */
function schemaKind(result: ResultSet, row: number, column: ColumnKind, cell: Cell): ValueKind | null {
  if (column.kind === "primaryKey") {
    const indexes = keyIndexes(result, column.columns);
    const target = indexes && rowAddress(result.rows[row], column.table, column.columns, indexes);
    return target ? { kind: "primaryKey", address: target } : null;
  }
  if (column.kind !== "reference") return null;
  const value = cellText(cell);
  if (value === null) return null;
  const targets = column.targets.flatMap((t) => targetAddress(t, value) ?? []);
  return { kind: "reference", targets, by: column.by, exists: null };
}

/** Rust sends the serialized cells sorted by row, then column. */
function findCell(cells: SerializedCell[], row: number, col: number): SerializedCell | null {
  let low = 0;
  let high = cells.length - 1;
  while (low <= high) {
    const mid = (low + high) >> 1;
    const c = cells[mid];
    const order = c.row - row || c.col - col;
    if (order === 0) return c;
    if (order < 0) low = mid + 1;
    else high = mid - 1;
  }
  return null;
}

/** Key column positions per result and key, resolved once instead of per cell. */
const keyIndexCache = new WeakMap<ResultSet, Map<string, number[] | null>>();

function keyIndexes(result: ResultSet, key: string[]): number[] | null {
  let byKey = keyIndexCache.get(result);
  if (!byKey) {
    byKey = new Map();
    keyIndexCache.set(result, byKey);
  }
  const id = key.join("\u0000");
  const cached = byKey.get(id);
  if (cached !== undefined) return cached;
  const names = result.columns.map((c) => c.name.toLowerCase());
  const indexes = key.map((column) => names.indexOf(column.toLowerCase()));
  const resolved = indexes.includes(-1) ? null : indexes;
  byKey.set(id, resolved);
  return resolved;
}

/** The table of a kind's table address; kinds always carry a `table` target. */
const tableName = (a: Address) => (a.target.kind === "table" ? a.target.table : null);

const withTarget = (base: Address, target: Target): Address => address(base.connection, base.database, target);

/** A single-column key addresses the row itself; a composite key is a filter on all of it. */
function rowAddress(cells: Cell[], table: Address, key: string[], indexes: number[]): Address | null {
  const name = tableName(table);
  if (name === null) return null;
  const values: string[] = [];
  for (const index of indexes) {
    const value = cells[index] ? cellText(cells[index]) : null;
    if (value === null) return null;
    values.push(value);
  }
  if (key.length === 1) return withTarget(table, { kind: "row", table: name, key: values[0] });
  const filter = key.map((column, i) => ({ column, value: values[i] }));
  return withTarget(table, { kind: "table", table: name, filter });
}

/** `null` for a target whose address is not a table, which the lens never builds. */
function targetAddress(target: ReferenceTarget, value: string): Address | null {
  const table = tableName(target.table);
  if (table === null) return null;
  if (target.key) return withTarget(target.table, { kind: "row", table, key: value });
  return withTarget(target.table, { kind: "table", table, filter: [{ column: target.column, value }] });
}
