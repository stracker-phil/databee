import { describe, expect, it } from "vitest";
import { centerOf, intersectRect, isInView, visiblePart } from "./rect";

const list = { x: 0, y: 100, width: 300, height: 200 };
const page = { x: 0, y: 0, width: 800, height: 600 };

describe("stage rectangles", () => {
  it("tells which part of an element its scroll areas let through", () => {
    expect(intersectRect({ x: 0, y: 0, width: 10, height: 10 }, { x: 5, y: 5, width: 10, height: 10 })).toEqual({ x: 5, y: 5, width: 5, height: 5 });
    expect(intersectRect({ x: 0, y: 0, width: 10, height: 10 }, { x: 10, y: 0, width: 5, height: 5 })).toBeNull();
    expect(visiblePart({ x: 10, y: 280, width: 100, height: 40 }, [list, page])).toEqual({ x: 10, y: 280, width: 100, height: 20 });
    expect(visiblePart({ x: 10, y: 400, width: 100, height: 40 }, [list, page])).toBeNull();
  });

  it("counts an element in view when half of it shows", () => {
    expect(isInView({ x: 10, y: 280, width: 100, height: 40 }, [list, page])).toBe(true);
    expect(isInView({ x: 10, y: 290, width: 100, height: 40 }, [list, page])).toBe(false);
    expect(isInView({ x: 10, y: 700, width: 100, height: 40 }, [page])).toBe(false);
  });

  it("counts a tall element in view when it fills half of its scroll area", () => {
    expect(isInView({ x: 0, y: 150, width: 300, height: 1000 }, [list, page])).toBe(true);
    expect(isInView({ x: 0, y: 250, width: 300, height: 1000 }, [list, page])).toBe(false);
  });

  it("finds the centre", () => {
    expect(centerOf({ x: 10, y: 20, width: 100, height: 40 })).toEqual({ x: 60, y: 40 });
  });
});
