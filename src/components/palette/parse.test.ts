import { describe, expect, it } from "vitest";
import { fuzzyScore, parsePrefix, score, searchesRows } from "./parse";

describe("searchesRows", () => {
  it.each([
    ["#", "12a", false],
    ["#", "12", true],
    ["@", "j", false],
    ["@", "jo", true],
    ["=", "ab", true],
    ["=", "a", false],
    [null, "short", false],
    [null, "longer", true],
  ] as const)("for prefix %s and %s answers %s", (prefix, needle, expected) => {
    expect(searchesRows(prefix, needle)).toBe(expected);
  });
});

describe("score", () => {
  it.each([
    ["an exact match", "orders", "orders", 4],
    ["a prefix", "orders_meta", "orders", 3],
    ["a contained needle", "wp_orders", "orders", 2],
    ["no match", "users", "orders", 0],
    ["an empty needle", "users", "", 1],
    ["a different case", "Orders", "oRDERS", 4],
  ])("scores %s", (_name, title, needle, expected) => {
    expect(score(title, needle)).toBe(expected);
  });
});

describe("parsePrefix", () => {
  it("reads c: as the connection switcher", () => {
    expect(parsePrefix("c: shop")).toEqual({ prefix: "c:", needle: "shop" });
  });
});

describe("fuzzyScore", () => {
  it.each([
    ["the best field's score", ["Shop", "shop"], "shop", 4],
    ["characters in order within one field", ["Staging", "127.0.0.1:3306"], "stgng", 1],
    ["characters spread over two fields", ["Shop", "127.0.0.1:32768"], "shop2", 0],
    ["an empty needle", ["Shop"], "", 1],
  ])("scores %s", (_name, fields, needle, expected) => {
    expect(fuzzyScore(fields, needle)).toBe(expected);
  });
});
