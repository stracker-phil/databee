import { describe, expect, it } from "vitest";
import { nextFocusIndex } from "./columns";

describe("nextFocusIndex", () => {
  it.each([
    ["the first value from no focus going down", -1, 1, 0],
    ["the last value from no focus going up", -1, -1, 4],
    ["stays on the last value going down", 4, 1, 4],
    ["stays on the first value going up", 0, -1, 0],
    ["moves one value down", 2, 1, 3],
    ["moves one value up", 2, -1, 1],
  ])("lands on %s", (_name, current, delta, expected) => {
    expect(nextFocusIndex(current, delta, 5)).toBe(expected);
  });
});
