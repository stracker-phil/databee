import { render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { reveal } from "./actions";

function stubLayout() {
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    const [x, y, width, height] = (this.getAttribute("data-rect") ?? "0,0,0,0").split(",").map(Number);
    return { x, y, width, height, top: y, left: x, right: x + width, bottom: y + height, toJSON: () => ({}) } as DOMRect;
  });
}

function renderInArea(anchorRect: string) {
  render(
    <div data-rect="0,0,300,200" style={{ overflowY: "auto" }}>
      <button data-rect={anchorRect}>Target</button>
    </div>,
  );
  const area = document.querySelector<HTMLElement>("div[data-rect]")!;
  Object.defineProperty(area, "clientWidth", { value: 300 });
  Object.defineProperty(area, "clientHeight", { value: 200 });
  const anchor = document.querySelector<HTMLElement>("button")!;
  anchor.scrollIntoView = vi.fn();
  return anchor;
}

describe("reveal", () => {
  beforeEach(stubLayout);
  afterEach(() => vi.restoreAllMocks());

  it("does not scroll an anchor that is already in view", async () => {
    const anchor = renderInArea("0,10,100,20");
    await reveal(anchor);
    expect(anchor.scrollIntoView).not.toHaveBeenCalled();
  });

  it("scrolls an anchor below the scroll area into view smoothly", async () => {
    const anchor = renderInArea("0,500,100,20");
    await reveal(anchor);
    expect(anchor.scrollIntoView).toHaveBeenCalledWith({ behavior: "smooth", block: "nearest", inline: "nearest" });
  });

  it("aligns the top of an anchor taller than its scroll area", async () => {
    const anchor = renderInArea("0,150,300,1000");
    await reveal(anchor);
    expect(anchor.scrollIntoView).toHaveBeenCalledWith({ behavior: "smooth", block: "start", inline: "nearest" });
  });
});
