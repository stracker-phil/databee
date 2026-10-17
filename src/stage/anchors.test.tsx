import { render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { clipsOf, collectAnchors } from "./anchors";

const MAX_LABEL = 80;

/** jsdom has no layout: elements take their box from a `data-rect="x,y,width,height"` attribute. */
function stubLayout() {
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    const [x, y, width, height] = (this.getAttribute("data-rect") ?? "0,0,0,0").split(",").map(Number);
    return { x, y, width, height, top: y, left: x, right: x + width, bottom: y + height, toJSON: () => ({}) };
  });
}

describe("anchors", () => {
  beforeEach(stubLayout);
  afterEach(() => vi.restoreAllMocks());

  describe("labelOf", () => {
    it("cuts a label longer than MAX_LABEL and ends it with an ellipsis", () => {
      const long = "a".repeat(MAX_LABEL + 20);
      render(<button data-stage="long" data-rect="0,0,10,10">{long}</button>);
      const [anchor] = collectAnchors();
      expect(anchor.label).toBe(`${"a".repeat(MAX_LABEL - 1)}…`);
      expect(anchor.label).toHaveLength(MAX_LABEL);
    });

    it("keeps a label of exactly MAX_LABEL characters", () => {
      const exact = "b".repeat(MAX_LABEL);
      render(<button data-stage="exact" data-rect="0,0,10,10">{exact}</button>);
      expect(collectAnchors()[0].label).toBe(exact);
    });

    it.each([
      ["data-stage-label on the element", <div key="a" data-stage="a" data-rect="0,0,10,10" data-stage-label="Own" aria-label="Aria">Text</div>, "Own"],
      ["data-stage-label inside the element", <div key="b" data-stage="b" data-rect="0,0,10,10" aria-label="Aria"><span data-stage-label="Inner">x</span></div>, "Inner"],
      ["aria-label", <div key="c" data-stage="c" data-rect="0,0,10,10" aria-label="Aria" title="Title">Text</div>, "Aria"],
      ["title", <div key="d" data-stage="d" data-rect="0,0,10,10" title="Title">Text</div>, "Title"],
      ["collapsed text", <div key="e" data-stage="e" data-rect="0,0,10,10">  Some   spaced{"\n"}text </div>, "Some spaced text"],
    ])("takes the label from %s", (_name, element, expected) => {
      render(element);
      expect(collectAnchors()[0].label).toBe(expected);
    });
  });

  describe("clipsOf", () => {
    it("clips an anchor by an overflow:auto parent, then by the window", () => {
      render(
        <div data-rect="10,20,300,200" style={{ overflowY: "auto" }}>
          <button data-stage="inside" data-rect="10,500,100,20">Inside</button>
        </div>,
      );
      const area = document.querySelector<HTMLElement>("[data-rect='10,20,300,200']")!;
      Object.defineProperty(area, "clientWidth", { value: 285 });
      Object.defineProperty(area, "clientHeight", { value: 200 });
      const anchor = document.querySelector<HTMLElement>("[data-stage]")!;
      expect(clipsOf(anchor)).toEqual([
        { x: 10, y: 20, width: 285, height: 200 },
        { x: 0, y: 0, width: window.innerWidth, height: window.innerHeight },
      ]);
    });

    it("ignores parents that do not scroll", () => {
      render(
        <div data-rect="10,20,300,200">
          <button data-stage="plain" data-rect="10,20,100,20">Plain</button>
        </div>,
      );
      expect(clipsOf(document.querySelector<HTMLElement>("[data-stage]")!)).toEqual([
        { x: 0, y: 0, width: window.innerWidth, height: window.innerHeight },
      ]);
    });

    it("reports an anchor below its scroll area as out of view", () => {
      render(
        <div data-rect="0,0,300,200" style={{ overflowY: "auto" }}>
          <button data-stage="below" data-rect="0,500,100,20">Below</button>
        </div>,
      );
      const area = document.querySelector<HTMLElement>("[data-rect='0,0,300,200']")!;
      Object.defineProperty(area, "clientWidth", { value: 300 });
      Object.defineProperty(area, "clientHeight", { value: 200 });
      expect(collectAnchors()[0]).toMatchObject({ id: "below", inView: false, visible: null });
    });
  });
});
