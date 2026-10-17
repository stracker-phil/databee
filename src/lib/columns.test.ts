import { describe, expect, it } from "vitest";
import { shownColumns } from "./columns";

describe("shownColumns", () => {
  it.each([
    ["ends at the newest column when the active one is the last", [3, 2, 1000], { start: 1, end: 3 }],
    ["starts at the first column when it is active", [3, 0, 1000], { start: 0, end: 2 }],
    ["shows exactly the active column when the width is too small", [4, 2, 100], { start: 2, end: 3 }],
    ["shows the only column when the width is too small", [1, 0, 300], { start: 0, end: 1 }],
  ] as const)("%s", (_name, [count, active, width], expected) => {
    expect(shownColumns(count, active, width)).toEqual(expected);
  });
});
