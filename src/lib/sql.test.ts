import { describe, expect, it } from "vitest";
import { statementAt, statementRanges } from "./sql";

const text = (sql: string, r: { from: number; to: number } | null) => (r ? sql.slice(r.from, r.to) : null);

describe("statementRanges", () => {
  it("splits on semicolons outside of strings and comments", () => {
    const sql = "SELECT 'a;b'; -- c;d\nSELECT `x;y` /* ; */;";
    expect(statementRanges(sql).map((r) => text(sql, r))).toEqual([
      "SELECT 'a;b'",
      "-- c;d\nSELECT `x;y` /* ; */",
    ]);
  });

  it("drops comment-only fragments", () => {
    expect(statementRanges("SELECT 1; -- end")).toHaveLength(1);
  });

  describe("double dashes", () => {
    const texts = (sql: string) => statementRanges(sql).map((r) => text(sql, r));

    it("keeps `--x` in the statement, since MySQL reads it as arithmetic, not a comment", () => {
      expect(texts("SELECT 1 --x")).toEqual(["SELECT 1 --x"]);
    });

    it("splits on a semicolon after `--x`, which hides nothing", () => {
      expect(texts("SELECT 1 --x;\nSELECT 2")).toEqual(["SELECT 1 --x", "SELECT 2"]);
    });

    it("hides a semicolon behind `-- x`, a comment", () => {
      expect(texts("SELECT 1 -- x;\nSELECT 2")).toEqual(["SELECT 1 -- x;\nSELECT 2"]);
    });

    it("reads `--` at the end of the input as a comment", () => {
      expect(texts("SELECT 1; --")).toEqual(["SELECT 1"]);
      expect(texts("--")).toEqual([]);
    });
  });
});

describe("statementAt", () => {
  const sql = "SELECT 1;\n\nSELECT 2;\nSELECT 3";

  it("picks the statement under the cursor", () => {
    expect(text(sql, statementAt(sql, sql.indexOf("2")))).toBe("SELECT 2");
  });

  it("treats the cursor right after a semicolon as part of that statement", () => {
    expect(text(sql, statementAt(sql, sql.indexOf(";") + 1))).toBe("SELECT 1");
  });

  it("falls back to the previous statement on blank lines", () => {
    expect(text(sql, statementAt(sql, sql.indexOf("\n\n") + 1))).toBe("SELECT 1");
  });

  it("prefers a non-empty selection", () => {
    expect(statementAt(sql, 0, { from: 0, to: 20 })).toEqual({ from: 0, to: 20 });
  });

  it("returns null for empty input", () => {
    expect(statementAt("  ", 0)).toBeNull();
  });
});
