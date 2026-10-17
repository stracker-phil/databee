import { describe, expect, it } from "vitest";
import type { Address, BrowsePage, Cell, ColumnKind, ResultSet } from "../api";
import { browsePage, columnMeta, num, resultSet, text } from "../test/fixtures";
import { resultValueAt, valueAt } from "./values";

const tableAddress = (table: string): Address => ({ connection: "local", database: "db", target: { kind: "table", table, filter: [] }, view: null, compare: null });
const rowAddress = (table: string, key: string): Address => ({ ...tableAddress(table), target: { kind: "row", table, key } });
const filterAddress = (table: string, filter: { column: string; value: string }[]): Address => ({ ...tableAddress(table), target: { kind: "table", table, filter } });

const COLUMNS = ["ID", "post_author", "post_parent", "post_title", "post_name", "guid", "menu_order", "post_date", "post_content"];

const kinds: ColumnKind[] = [
  { kind: "primaryKey", table: tableAddress("wp_posts"), columns: ["ID"] },
  { kind: "reference", targets: [{ table: tableAddress("wp_users"), column: "ID", key: true }], by: { kind: "foreignKey", constraint: "fk_author" } },
  { kind: "reference", targets: [{ table: tableAddress("wp_posts"), column: "post_parent", key: false }], by: { kind: "sameName", column: "post_parent", indexed: true } },
  { kind: "text" },
  { kind: "text" },
  { kind: "text" },
  { kind: "number" },
  { kind: "timestamp" },
  { kind: "text" },
];

const rows: Cell[][] = [
  [num("7"), num("3"), num("12"), text("Hello"), text(""), { t: "bytes", v: { len: 4, hexPreview: "dead" } }, num("2"), { t: "temporal", v: "2024-01-01 10:00:00" }, text('a:1:{s:1:"k";s:1:"v";}')],
  [num("8"), num("0"), { t: "null" }, { t: "null" }, text("slug"), { t: "null" }, num("0"), { t: "null" }, text("plain")],
];

const page: BrowsePage = browsePage({
  table: "wp_posts",
  result: resultSet({
    columns: COLUMNS.map((name) => columnMeta(name, "TEXT")),
    rows,
    totalRows: 2,
    kinds,
    serialized: [{ row: 0, col: 8, format: "php", preview: "a:1:{…}" }],
  }),
  keys: [[{ column: "ID", value: "7" }], [{ column: "ID", value: "8" }]],
});

const at = (row: string, column: string, p: BrowsePage = page) => valueAt(p, row === "first" ? 0 : 1, COLUMNS.indexOf(column));

describe("valueAt", () => {
  it("returns a primary key cell with the address of its row", () => {
    expect(at("first", "ID").kind).toEqual({ kind: "primaryKey", address: rowAddress("wp_posts", "7") });
  });

  it("keeps the raw cell", () => {
    expect(at("first", "post_title").raw).toEqual(text("Hello"));
  });

  it("addresses a reference to a key column as the target row", () => {
    expect(at("first", "post_author").kind).toEqual({
      kind: "reference",
      targets: [rowAddress("wp_users", "3")],
      by: { kind: "foreignKey", constraint: "fk_author" },
      exists: null,
    });
  });

  it("addresses a reference to a non-key column as a filter on the target table", () => {
    expect(at("first", "post_parent").kind).toEqual({
      kind: "reference",
      targets: [filterAddress("wp_posts", [{ column: "post_parent", value: "12" }])],
      by: { kind: "sameName", column: "post_parent", indexed: true },
      exists: null,
    });
  });

  it("marks a reference holding 0 as noReference", () => {
    expect(at("second", "post_author").kind).toEqual({ kind: "noReference" });
  });

  it("marks a null reference as noReference", () => {
    expect(at("second", "post_parent").kind).toEqual({ kind: "noReference" });
  });

  it("distinguishes an empty string from null", () => {
    expect(at("first", "post_name").kind).toEqual({ kind: "empty" });
    expect(at("second", "post_title").kind).toEqual({ kind: "null" });
  });

  it("marks bytes as binary", () => {
    expect(at("first", "guid").kind).toEqual({ kind: "binary" });
  });

  it("marks a decoded cell as serialized with its format and preview", () => {
    expect(at("first", "post_content").kind).toEqual({ kind: "serialized", format: "php", preview: "a:1:{…}" });
  });

  it("leaves a cell without a serialized entry as text", () => {
    expect(at("second", "post_content").kind).toEqual({ kind: "text" });
  });

  it("marks number and timestamp columns", () => {
    expect(at("first", "menu_order").kind).toEqual({ kind: "number" });
    expect(at("first", "post_date").kind).toEqual({ kind: "timestamp", utc: null });
  });

  it("marks a null timestamp as null", () => {
    expect(at("second", "post_date").kind).toEqual({ kind: "null" });
  });

  it("falls back to text when the result has no column kinds", () => {
    const bare = browsePage({ ...page, result: { ...page.result, kinds: [], serialized: [] } });
    expect(at("first", "menu_order", bare).kind).toEqual({ kind: "text" });
  });

  describe("source", () => {
    it("is the page table, the column and the key of the row", () => {
      expect(at("second", "post_title").source).toEqual({ table: "wp_posts", column: "post_title", row: [{ column: "ID", value: "8" }] });
    });

    it("is null when the table has no primary key", () => {
      expect(at("first", "post_title", { ...page, keys: [] }).source).toBeNull();
    });
  });
});

describe("resultValueAt", () => {
  const composite: ResultSet = resultSet({
    columns: [columnMeta("a", "INT"), columnMeta("b", "INT")],
    rows: [[num("1"), num("2")]],
    kinds: [
      { kind: "primaryKey", table: tableAddress("wp_term_relationships"), columns: ["a", "b"] },
      { kind: "primaryKey", table: tableAddress("wp_term_relationships"), columns: ["a", "b"] },
    ],
  });

  it("has no source without one", () => {
    expect(resultValueAt(page.result, 0, 0).source).toBeNull();
  });

  it("carries the given source", () => {
    const source = { table: "t", column: "ID", row: [{ column: "ID", value: "7" }] };
    expect(resultValueAt(page.result, 0, 0, source).source).toEqual(source);
  });

  it("addresses a composite primary key as a filter on all its columns", () => {
    expect(resultValueAt(composite, 0, 0).kind).toEqual({
      kind: "primaryKey",
      address: filterAddress("wp_term_relationships", [
        { column: "a", value: "1" },
        { column: "b", value: "2" },
      ]),
    });
  });

  it("falls back to the cell's own kind when a key column is missing from the result", () => {
    const partial = resultSet({
      columns: [columnMeta("a", "INT")],
      rows: [[num("1")]],
      kinds: [{ kind: "primaryKey", table: tableAddress("wp_term_relationships"), columns: ["a", "b"] }],
    });
    expect(resultValueAt(partial, 0, 0).kind).toEqual({ kind: "text" });
  });
});
