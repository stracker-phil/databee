import { describe, expect, it } from "vitest";
import type { Coverage } from "../api";
import { coverageProblem, unresolvedCount } from "./coverageProblems";

const coverage = (patch: Partial<Coverage> = {}): Coverage => ({ total: 0, resolved: 0, zeroOrNull: 0, sampleUnresolved: [], timedOut: false, ...patch });

const rows = { unresolved: "`ref` NOT IN (0)", zeroOrNull: "`ref` = 0 OR `ref` IS NULL" };

describe("unresolvedCount", () => {
  it("counts the values that hold something but find no target", () => {
    expect(unresolvedCount(coverage({ total: 40, resolved: 30, zeroOrNull: 4 }))).toBe(6);
  });
});

describe("coverageProblem", () => {
  it.each([
    ["no rows were counted", coverage()],
    ["every value resolves", coverage({ total: 7, resolved: 7 })],
    ["most values are 0 or NULL", coverage({ total: 100, resolved: 11, zeroOrNull: 89 })],
    ["all values are 0 or NULL", coverage({ total: 100, zeroOrNull: 100 })],
  ])("finds no problem when %s", (_what, c) => {
    expect(coverageProblem(c)).toBeNull();
  });

  it("names the values that point nowhere with a sample", () => {
    const problem = coverageProblem(coverage({ total: 40, resolved: 35, sampleUnresolved: ["98", "99"] }));
    expect(problem?.text).toBe("5 of 40 point nowhere (98, 99)");
  });

  it("shows at most three sample values", () => {
    const problem = coverageProblem(coverage({ total: 10, resolved: 5, sampleUnresolved: ["1", "2", "3", "4", "5"] }));
    expect(problem?.text).toBe("5 of 10 point nowhere (1, 2, 3)");
  });

  it("omits the sample when none was kept", () => {
    expect(coverageProblem(coverage({ total: 10, resolved: 9 }))?.text).toBe("1 of 10 point nowhere");
  });

  it("reports only the unresolved values when many others are 0 or NULL", () => {
    const problem = coverageProblem(coverage({ total: 100, resolved: 5, zeroOrNull: 91, sampleUnresolved: ["7"] }));
    expect(problem?.text).toBe("4 of 100 point nowhere (7)");
  });

  describe("filter", () => {
    it("is the unresolved rows", () => {
      expect(coverageProblem(coverage({ total: 100, resolved: 5, zeroOrNull: 91, rows }))?.filter).toBe(rows.unresolved);
    });

    it("is null without rows", () => {
      expect(coverageProblem(coverage({ total: 10, resolved: 5 }))?.filter).toBeNull();
    });
  });
});
