/**
 * A cell as a `Value` (kind, provenance and style), built from the result's column kinds, per-cell
 * kinds, value styles and serialized cells instead of one `Value` per cell from Rust (payload size). A port of Rust's
 * `value::value_at`, the reference implementation; tests keep both equal.
 */
import type { Address, BrowsePage, Cell, ColumnKind, Provenance, ReferenceTarget, ResultSet, Target, Value, ValueKind, Zone } from "../api";
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
  const column: ColumnKind = findAt(result.cellKinds, row, col)?.kind ?? result.kinds[col] ?? { kind: "text" };
  const style = findAt(result.styles, row, col)?.style ?? null;
  return { raw, kind: cellKind(result, row, col, raw, column, source), source, style, secondary: null };
}

/** A cell outside any result's kinds (no lens): only what the cell itself says. */
export function cellValue(raw: Cell): Value {
  return { raw, kind: plainKind(raw), source: null, style: null, secondary: null };
}

function plainKind(cell: Cell): ValueKind {
  if (cell.t === "null") return { kind: "null" };
  if (cell.t === "bytes") return { kind: "binary" };
  if (cell.t === "text" && cell.v === "") return { kind: "empty" };
  return cell.t === "number" ? { kind: "number" } : { kind: "text" };
}

const cellText = (cell: Cell): string | null => (cell.t === "number" || cell.t === "text" || cell.t === "temporal" || cell.t === "json" ? cell.v : null);

/** `0` stores "no target"; NULL means the same in a reference column. */
const isNoReference = (cell: Cell) => cell.t === "null" || cellText(cell) === "0";

function cellKind(result: ResultSet, row: number, col: number, cell: Cell, column: ColumnKind, source: Provenance | null): ValueKind {
  if (column.kind === "reference" && isNoReference(cell)) return { kind: "noReference" };
  if (cell.t === "null") return { kind: "null" };
  if (cell.t === "text" && cell.v === "") return { kind: "empty" };
  if (cell.t === "bytes") return { kind: "binary" };
  // A secret stays masked even when it decodes.
  if (column.kind === "secret") return { kind: "secret" };

  const fromSchema = schemaKind(result, row, column, cell);
  if (fromSchema) return fromSchema;
  const serialized = findAt(result.serialized, row, col);
  if (serialized) return { kind: "serialized", format: serialized.format, preview: serialized.preview };
  switch (column.kind) {
    case "number":
      return { kind: "number" };
    case "timestamp": {
      const text = cellText(cell);
      return { kind: "timestamp", utc: text === null ? null : utcSeconds(text, column.zone), zone: column.zone };
    }
    case "money":
      return { kind: "money", currency: currency(result, row, column.currency, source) };
    case "enum":
      return { kind: "enum" };
    default:
      return { kind: "text" };
  }
}

/** The first of `names` that is a column of the result with text in this row, sourced from the same row. */
function currency(result: ResultSet, row: number, names: string[], source: Provenance | null): Value | null {
  for (const name of names) {
    const index = result.columns.findIndex((c) => c.name.toLowerCase() === name.toLowerCase());
    if (index < 0) continue;
    const raw = result.rows[row][index];
    const text = cellText(raw);
    if (!text) continue;
    const from = source ? { ...source, column: result.columns[index].name } : null;
    return { raw, kind: { kind: "text" }, source: from, style: null, secondary: null };
  }
  return null;
}

/**
 * Unix seconds of `text` read in `zone`: `null` for a zone that is not stored (`local`, none), zero dates
 * and text that is no time. A port of Rust's `value::time::utc_seconds`.
 */
export function utcSeconds(text: string, zone: Zone | null): number | null {
  if (zone === "unix") {
    const digits = /^\d+/.exec(text)?.[0];
    const seconds = digits ? Number(digits) : 0;
    return seconds === 0 ? null : seconds;
  }
  if (zone !== "utc") return null;
  const full = /^(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2}):(\d{2})/.exec(text);
  const atTime = full ? utcOf(full.slice(1).map(Number)) : null;
  if (atTime !== null) return atTime;
  const date = /^(\d{4})-(\d{2})-(\d{2})/.exec(text);
  return date ? utcOf([...date.slice(1).map(Number), 0, 0, 0]) : null;
}

/** `null` for a field out of range (month or day 0, 24 o'clock), which `Date.UTC` would roll over. */
function utcOf([year, month, day, hour, minute, second]: number[]): number | null {
  const ms = Date.UTC(year, month - 1, day, hour, minute, second);
  const at = new Date(ms);
  const exact =
    at.getUTCFullYear() === year && at.getUTCMonth() === month - 1 && at.getUTCDate() === day && at.getUTCHours() === hour && at.getUTCMinutes() === minute && at.getUTCSeconds() === second;
  return exact ? ms / 1000 : null;
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

/** Rust sends per-cell lists sorted by row, then column. */
function findAt<T extends { row: number; col: number }>(cells: T[], row: number, col: number): T | null {
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

/** The filter a table address already carries (a reference's `to_when`). */
const ownFilter = (a: Address) => (a.target.kind === "table" ? a.target.filter : []);

/** `null` for a target whose address is not a table, which the lens never builds. */
function targetAddress(target: ReferenceTarget, value: string): Address | null {
  const table = tableName(target.table);
  if (table === null) return null;
  if (target.key) return withTarget(target.table, { kind: "row", table, key: value });
  return withTarget(target.table, { kind: "table", table, filter: [...ownFilter(target.table), { column: target.column, value }] });
}
