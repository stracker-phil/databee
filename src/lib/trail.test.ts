import { describe, expect, it } from "vitest";
import { foldSteps, trailUnits, type TrailUnit } from "./trail";

const table: TrailUnit = { kind: "table" };
const c = (index: number): TrailUnit => ({ kind: "column", index });
const before = (columns: number[], withTable = false): TrailUnit => ({ kind: "more", side: "before", table: withTable, columns });
const after = (columns: number[]): TrailUnit => ({ kind: "more", side: "after", table: false, columns });

describe("trailUnits", () => {
  describe("six columns, panes 3 and 4, with the table", () => {
    it("counts a fold step per hidden column and one for the table", () => {
      expect(foldSteps(6, 3, 5, true)).toBe(5);
      expect(foldSteps(6, 3, 5, false)).toBe(4);
    });

    it.each([
      [0, [table, c(0), c(1), c(2), c(3), c(4), c(5)]],
      [1, [table, c(0), before([1]), c(2), c(3), c(4), c(5)]],
      [2, [table, c(0), before([1, 2]), c(3), c(4), c(5)]],
      [3, [table, before([0, 1, 2]), c(3), c(4), c(5)]],
      [4, [table, before([0, 1, 2]), c(3), c(4), after([5])]],
      [5, [before([0, 1, 2], true), c(3), c(4), after([5])]],
    ])("after %i folds", (steps, expected) => {
      expect(trailUnits(6, 3, 5, true, steps)).toEqual(expected);
    });
  });

  it("never folds the columns shown as panes", () => {
    const units = trailUnits(6, 3, 5, true, foldSteps(6, 3, 5, true));
    expect(units).toContainEqual(c(3));
    expect(units).toContainEqual(c(4));
  });

  it("starts with a ... holding only the table when no column is left of the panes", () => {
    expect(trailUnits(3, 0, 1, true, 2)).toEqual([table, c(0), after([1, 2])]);
    expect(trailUnits(3, 0, 1, true, 3)).toEqual([before([], true), c(0), after([1, 2])]);
  });

  it("has no table unit for a root that is not a row", () => {
    expect(trailUnits(2, 0, 2, false, 0)).toEqual([c(0), c(1)]);
  });
});
