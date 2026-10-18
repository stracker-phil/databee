import { describe, expect, it } from "vitest";
import { address } from "./address";
import { maximizable, objectColumn, paneShares, shownColumns } from "./columns";

describe("shownColumns", () => {
  it.each([
    ["ends at the newest column when the active one is the last", [3, 2, 1200, 2], { start: 1, end: 3 }],
    ["starts at the first column when it is active", [3, 0, 1200, 2], { start: 0, end: 2 }],
    ["lets the active column further left start the panes", [5, 1, 5000, 2], { start: 1, end: 3 }],
    ["shows exactly the active column when the width is too small", [4, 2, 100, 3], { start: 2, end: 3 }],
    ["shows the only column when the width is too small", [1, 0, 300, 2], { start: 0, end: 1 }],
    ["shows every column when there are fewer than the setting allows", [2, 1, 5000, 3], { start: 0, end: 2 }],
    ["measures nothing before the width is known", [3, 2, Infinity, 2], { start: 1, end: 3 }],
  ] as const)("%s", (_name, [count, active, width, maxPanes], expected) => {
    expect(shownColumns(count, active, width, maxPanes)).toEqual(expected);
  });

  it.each([
    [1, { start: 4, end: 5 }],
    [2, { start: 3, end: 5 }],
    [3, { start: 2, end: 5 }],
  ])("shows at most maxPanes %i columns of a wide window", (maxPanes, expected) => {
    expect(shownColumns(5, 4, 5000, maxPanes)).toEqual(expected);
  });

  it.each([
    [1120, { start: 4, end: 5 }],
    [1121, { start: 3, end: 5 }],
    [1681, { start: 3, end: 5 }],
    [1682, { start: 2, end: 5 }],
  ])("fits panes of 560px and a 1px divider into a width of %i", (width, expected) => {
    expect(shownColumns(5, 4, width, 3)).toEqual(expected);
  });
});

describe("maximizable", () => {
  const stack = (count: number, maximized?: boolean) => ({
    columns: Array.from({ length: count }, (_, i) => objectColumn(address("shop", "db", { kind: "row", table: "wp_posts", key: String(i) }))),
    maximized,
  });

  it.each([
    ["several columns while several panes are allowed", stack(2), 2, true],
    ["not a single column", stack(1), 2, false],
    ["not when only one pane is allowed", stack(2), 1, false],
    ["a maximized stack even when only one pane is allowed, so it can be restored", stack(2, true), 1, true],
  ])("%s", (_name, columns, maxPanes, expected) => {
    expect(maximizable(columns, maxPanes)).toBe(expected);
  });
});

describe("paneShares", () => {
  it("uses the widths when there is one per pane", () => {
    expect(paneShares([0.3, 0.7], 2)).toEqual([0.3, 0.7]);
  });

  it.each([
    ["no widths are stored", undefined, 2, [0.5, 0.5]],
    ["the stored widths are for another number of panes", [0.3, 0.7], 3, [1 / 3, 1 / 3, 1 / 3]],
  ])("shares equally when %s", (_name, widths, count, expected) => {
    expect(paneShares(widths, count)).toEqual(expected);
  });
});
