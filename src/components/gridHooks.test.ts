import { describe, expect, it } from "vitest";
import { columnMeta, text } from "../test/fixtures";
import { fitWidth } from "./gridHooks";

describe("fitWidth", () => {
  const col = columnMeta("id", "BIGINT");
  const rows = [[text("x".repeat(30))]];
  const fit = (lead?: number, cap = 380) => fitWidth(col, 0, rows, "NULL", 12, cap, lead);

  it("is wider by the lead for a column whose values start with an icon", () => {
    expect(fit(2.5)).toBeGreaterThan(fit());
    expect(fit(2.5) - fit()).toBe(Math.round(2.5 * 12 * 0.6));
  });

  it("treats a lead of 0 like no lead", () => {
    expect(fit(0)).toBe(fit());
  });

  it("still stops at the cap", () => {
    expect(fit(2.5, 100)).toBe(100);
  });
});
