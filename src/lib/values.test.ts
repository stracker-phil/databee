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
  { kind: "timestamp", zone: null },
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
    expect(at("first", "post_date").kind).toEqual({ kind: "timestamp", utc: null, zone: null });
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

  describe("cellKinds", () => {
    const overridden = resultSet({
      columns: [columnMeta("a", "TEXT"), columnMeta("b", "TEXT")],
      rows: [
        [text("x"), text("1")],
        [text("y"), text("2")],
      ],
      kinds: [{ kind: "text" }, { kind: "text" }],
      cellKinds: [{ row: 0, col: 1, kind: { kind: "number" } }],
    });

    it("overrides the column kind for that cell", () => {
      expect(resultValueAt(overridden, 0, 1).kind).toEqual({ kind: "number" });
    });

    it("leaves the other cells of the column and row on the column kind", () => {
      expect(resultValueAt(overridden, 1, 1).kind).toEqual({ kind: "text" });
      expect(resultValueAt(overridden, 0, 0).kind).toEqual({ kind: "text" });
    });
  });

  describe("styles", () => {
    const styled = resultSet({
      columns: [columnMeta("a", "TEXT")],
      rows: [[text("ok")], [text("failed")]],
      styles: [{ row: 1, col: 0, style: "danger" }],
    });

    it("sets the style of a styled cell", () => {
      expect(resultValueAt(styled, 1, 0).style).toBe("danger");
    });

    it("leaves an unstyled cell without a style", () => {
      expect(resultValueAt(styled, 0, 0).style).toBeNull();
    });
  });

  describe("money", () => {
    const priced = (currencyNames: string[], currencyCell: Cell | null = text("EUR")) =>
      resultSet({
        columns: [columnMeta("total", "DECIMAL"), columnMeta("Currency", "VARCHAR")],
        rows: [currencyCell ? [num("9.50"), currencyCell] : [num("9.50"), { t: "null" }]],
        kinds: [{ kind: "money", currency: currencyNames }, { kind: "text" }],
      });
    const source = { table: "orders", column: "total", row: [{ column: "id", value: "1" }] };

    it("takes the currency from the named column of the same row, matched case-insensitively", () => {
      expect(resultValueAt(priced(["currency"]), 0, 0, source).kind).toEqual({
        kind: "money",
        currency: { raw: text("EUR"), kind: { kind: "text" }, source: { ...source, column: "Currency" }, style: null, secondary: null },
      });
    });

    it("uses the first listed column that exists", () => {
      const kind = resultValueAt(priced(["missing", "currency"]), 0, 0).kind;
      expect(kind).toMatchObject({ kind: "money", currency: { raw: text("EUR"), source: null } });
    });

    it.each([
      ["names no column of the result", ["missing"], text("EUR")],
      ["is null in the row", ["currency"], null],
      ["is empty in the row", ["currency"], text("")],
    ])("has no currency when the currency column %s", (_name, names, cell) => {
      expect(resultValueAt(priced(names, cell), 0, 0).kind).toEqual({ kind: "money", currency: null });
    });
  });

  describe("timestamp", () => {
    const stamped = (zone: "unix" | "utc" | "local" | null, cell: Cell) =>
      resultValueAt(resultSet({ columns: [columnMeta("t", "DATETIME")], rows: [[cell]], kinds: [{ kind: "timestamp", zone }] }), 0, 0).kind;

    it("carries the zone and the UTC seconds of a UTC datetime", () => {
      expect(stamped("utc", { t: "temporal", v: "2024-01-01 10:00:00" })).toEqual({ kind: "timestamp", utc: 1704103200, zone: "utc" });
    });

    it("keeps the zone and no seconds for a local datetime", () => {
      expect(stamped("local", { t: "temporal", v: "2024-01-01 10:00:00" })).toEqual({ kind: "timestamp", utc: null, zone: "local" });
    });

    it("reads a unix timestamp stored as a number", () => {
      expect(stamped("unix", { t: "number", v: "1700000000" })).toEqual({ kind: "timestamp", utc: 1700000000, zone: "unix" });
    });
  });

  describe("secret", () => {
    it("masks a secret that decodes as serialized text", () => {
      const secret = resultSet({
        columns: [columnMeta("token", "TEXT")],
        rows: [[text('a:1:{s:1:"k";s:1:"v";}')]],
        kinds: [{ kind: "secret" }],
        serialized: [{ row: 0, col: 0, format: "php", preview: "a:1:{…}" }],
      });
      expect(resultValueAt(secret, 0, 0).kind).toEqual({ kind: "secret" });
    });

    it("still shows null as null", () => {
      const secret = resultSet({ columns: [columnMeta("token", "TEXT")], rows: [[{ t: "null" }]], kinds: [{ kind: "secret" }] });
      expect(resultValueAt(secret, 0, 0).kind).toEqual({ kind: "null" });
    });
  });

  describe("reference target with its own filter", () => {
    it("adds the referenced value to the filter the target already carries", () => {
      const own = [{ column: "post_type", value: "product" }];
      const result = resultSet({
        columns: [columnMeta("product_id", "BIGINT")],
        rows: [[num("5")]],
        kinds: [{ kind: "reference", targets: [{ table: filterAddress("wp_posts", own), column: "post_parent", key: false }], by: { kind: "sameName", column: "post_parent", indexed: true } }],
      });
      expect(resultValueAt(result, 0, 0).kind).toMatchObject({
        kind: "reference",
        targets: [
          filterAddress("wp_posts", [
            { column: "post_type", value: "product" },
            { column: "post_parent", value: "5" },
          ]),
        ],
      });
    });
  });
});

describe("timestamp seconds", () => {
  const utcSeconds = (input: string, zone: "unix" | "utc" | "local" | null) => {
    const result = resultSet({ columns: [columnMeta("t", "DATETIME")], rows: [[text(input)]], kinds: [{ kind: "timestamp", zone }] });
    const kind = resultValueAt(result, 0, 0).kind;
    return kind.kind === "timestamp" ? kind.utc : undefined;
  };

  it.each([
    ["unix", "1700000000", 1700000000],
    ["unix", "1700000000:42", 1700000000],
    ["unix", "0", null],
    ["unix", "0:42", null],
    ["unix", "soon", null],
    ["utc", "2024-01-01 10:00:00", 1704103200],
    ["utc", "2024-01-01 10:00:00.123456", 1704103200],
    ["utc", "2024-01-01", 1704067200],
    ["utc", "0000-00-00 00:00:00", null],
    ["utc", "0000-00-00", null],
    ["utc", "2024-13-01 00:00:00", null],
    ["utc", "never", null],
    ["local", "2024-01-01 10:00:00", null],
    [null, "2024-01-01 10:00:00", null],
  ] as const)("reads %s text %j as %s", (zone, input, expected) => {
    expect(utcSeconds(input, zone)).toBe(expected);
  });
});
