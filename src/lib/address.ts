/**
 * Addresses (`databee://<slug>/<database>/<table>/<key>?…`), ported from Rust's `address` module so
 * every navigation can build one without a round trip. `src/test/fixtures/addresses.json`, written by
 * the Rust tests, keeps both sides equal.
 */
import type { Address, ColumnEquals, Target } from "../api";

export const SCHEME = "databee://";
/** The path segment of saved queries; a table literally named `q` is written `%71`. */
const QUERY_SEGMENT = "q";
const UNRESERVED = /[A-Za-z0-9\-._~]/;
const utf8 = new TextEncoder();

export class AddressError extends Error {}

/** Everything but `A-Z a-z 0-9 - . _ ~`, as uppercase UTF-8 escapes (unlike `encodeURIComponent`, also `!'()*`). */
export function encodePart(text: string): string {
  let out = "";
  for (const char of text) {
    if (UNRESERVED.test(char)) {
      out += char;
      continue;
    }
    for (const byte of utf8.encode(char)) out += `%${byte.toString(16).toUpperCase().padStart(2, "0")}`;
  }
  return out;
}

/** Why a text is not an address; `parseAddress` adds the text itself. */
class Invalid extends Error {}

const invalid = (reason: string) => new Invalid(reason);

function decodePart(text: string): string {
  if (/%(?![0-9A-Fa-f]{2})/.test(text)) throw invalid(`broken percent escape in \`${text}\``);
  try {
    return decodeURIComponent(text);
  } catch {
    throw invalid(`\`${text}\` does not decode to UTF-8 text`);
  }
}

function nonEmpty(raw: string, part: string): string {
  const decoded = decodePart(raw);
  if (!decoded) throw invalid(`the ${part} is empty`);
  return decoded;
}

/** An address without view or compare parameters. */
export const address = (connection: string, database: string, target: Target): Address => ({ connection, database, target, view: null, compare: null });

/** A parameter without `=` (`?project`, a NULL filter `?col`) has a null value. */
type Params = [name: string, value: string | null][];

/** `a=1&b` → `[["a", "1"], ["b", null]]`. */
function parseQuery(query: string): Params {
  if (!query) return [];
  return query.split("&").map((pair) => {
    const at = pair.indexOf("=");
    const name = at < 0 ? pair : pair.slice(0, at);
    const value = at < 0 ? null : decodePart(pair.slice(at + 1));
    return [nonEmpty(name, "parameter name"), value];
  });
}

function homeTarget(params: Params): Target {
  if (params.length === 0) return { kind: "home" };
  if (params.length === 1) {
    const [[name, value]] = params;
    if (name === "find" && value !== null) return { kind: "find", value };
    const isProjectFlag = name === "project" && value === null;
    if (isProjectFlag) return { kind: "project" };
  }
  throw invalid("the database home takes only ?find=<value> or ?project");
}

function withRowParams(row: Address, params: Params): Address {
  let view: Address["view"] = null;
  let compare: string | null = null;
  for (const [name, value] of params) {
    if (name === "view" && value !== null && view === null) {
      if (value !== "document" && value !== "rows") throw invalid("view is document or rows");
      view = value;
    } else if (name === "compare" && value !== null && value !== "" && compare === null) {
      compare = value;
    } else {
      throw invalid(`\`${name}\` is not a row parameter or is repeated`);
    }
  }
  return { ...row, view, compare };
}

function parseParts(text: string): Address {
  if (!text.startsWith(SCHEME)) throw invalid("it does not start with databee://");
  const rest = text.slice(SCHEME.length);
  const at = rest.indexOf("?");
  const path = at < 0 ? rest : rest.slice(0, at);
  const params = parseQuery(at < 0 ? "" : rest.slice(at + 1));
  const [rawConnection, rawDatabase, ...tail] = path.split("/");
  if (rawDatabase === undefined) throw invalid("it needs a connection and a database");
  const connection = nonEmpty(rawConnection, "connection");
  const database = nonEmpty(rawDatabase, "database");

  if (tail.length === 0 || (tail.length === 1 && tail[0] === "")) return address(connection, database, homeTarget(params));
  if (tail.length === 2 && tail[0] === QUERY_SEGMENT) {
    if (params.length > 0) throw invalid("a saved query takes no parameters");
    return address(connection, database, { kind: "query", id: nonEmpty(tail[1], "query id") });
  }
  if (tail.length === 1) {
    const filter: ColumnEquals[] = params.map(([column, value]) => ({ column, value }));
    return address(connection, database, { kind: "table", table: nonEmpty(tail[0], "table"), filter });
  }
  if (tail.length === 2) {
    const target: Target = { kind: "row", table: nonEmpty(tail[0], "table"), key: nonEmpty(tail[1], "key") };
    return withRowParams(address(connection, database, target), params);
  }
  throw invalid("it has too many path segments");
}

/** Throws an `AddressError` for anything Rust's `Address::parse` rejects, with the same message. */
export function parseAddress(text: string): Address {
  try {
    return parseParts(text);
  } catch (e) {
    if (e instanceof Invalid) throw new AddressError(`Not a DataBee address (${e.message}): ${text}`);
    throw e;
  }
}

const tableSegment = (table: string) => (table === QUERY_SEGMENT ? "%71" : encodePart(table));
const param = (name: string, value: string | null) => (value === null ? encodePart(name) : `${encodePart(name)}=${encodePart(value)}`);

function pathAndParams(a: Address): [string, string[]] {
  const t = a.target;
  switch (t.kind) {
    case "home":
      return ["", []];
    case "table":
      return [tableSegment(t.table), t.filter.map((c) => param(c.column, c.value))];
    case "row": {
      const params: string[] = [];
      if (a.view !== null) params.push(param("view", a.view));
      if (a.compare !== null) params.push(param("compare", a.compare));
      return [`${tableSegment(t.table)}/${encodePart(t.key)}`, params];
    }
    case "query":
      return [`${QUERY_SEGMENT}/${encodePart(t.id)}`, []];
    case "find":
      return ["", [param("find", t.value)]];
    case "project":
      return ["", [param("project", null)]];
  }
}

/** The canonical text, equal to Rust's `Display`. */
export function formatAddress(a: Address): string {
  const [path, params] = pathAndParams(a);
  const query = params.length > 0 ? `?${params.join("&")}` : "";
  return `${SCHEME}${encodePart(a.connection)}/${encodePart(a.database)}/${path}${query}`;
}

export const sameAddress = (a: Address, b: Address) => formatAddress(a) === formatAddress(b);
