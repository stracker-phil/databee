import { describe, expect, it } from "vitest";
import { score, searchesRows } from "./parse";

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
