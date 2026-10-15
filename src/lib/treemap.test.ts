import { describe, expect, it } from "vitest";
import { squarify } from "./treemap";

const area = (r: { w: number; h: number }) => r.w * r.h;

describe("squarify", () => {
  it("fills the box with areas proportional to values", () => {
    const rects = squarify(
      [6, 6, 4, 3, 2, 2, 1].map((v, i) => ({ value: v, data: i })),
      600,
      400,
    );
    expect(rects).toHaveLength(7);
    const total = rects.reduce((s, r) => s + area(r), 0);
    expect(total).toBeCloseTo(600 * 400, 3);
    expect(area(rects[0]) / area(rects[6])).toBeCloseTo(6, 3);
  });

  it("keeps every tile inside the box", () => {
    const rects = squarify([50, 30, 10, 5, 5].map((v) => ({ value: v, data: v })), 300, 200);
    for (const r of rects) {
      expect(r.x).toBeGreaterThanOrEqual(-1e-9);
      expect(r.y).toBeGreaterThanOrEqual(-1e-9);
      expect(r.x + r.w).toBeLessThanOrEqual(300 + 1e-9);
      expect(r.y + r.h).toBeLessThanOrEqual(200 + 1e-9);
    }
  });

  it("puts the largest value first and drops zeros", () => {
    const rects = squarify([{ value: 1, data: "small" }, { value: 0, data: "zero" }, { value: 9, data: "big" }], 100, 100);
    expect(rects.map((r) => r.data)).toEqual(["big", "small"]);
  });

  it("returns nothing for empty input", () => {
    expect(squarify([], 100, 100)).toEqual([]);
  });
});
