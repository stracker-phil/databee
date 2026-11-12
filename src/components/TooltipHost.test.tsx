import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { installCommands } from "../commands";
import { resetStore, useStore } from "../state/store";
import { TooltipHost } from "./TooltipHost";

function renderHost() {
  render(
    <>
      <button type="button" data-tooltip="Refresh the list" />
      <button type="button" data-tooltip="Sort the rows">
        Sort
      </button>
      <button type="button" data-tooltip="Open the settings" />
      <span>No tip here</span>
      <TooltipHost />
    </>,
  );
}

const iconOnly = () => document.querySelector<HTMLElement>('[data-tooltip="Refresh the list"]')!;
const second = () => document.querySelector<HTMLElement>('[data-tooltip="Open the settings"]')!;
const nothing = () => screen.getByText("No tip here");
const wait = (ms: number) => act(() => vi.advanceTimersByTimeAsync(ms));

beforeEach(() => {
  vi.useFakeTimers();
  resetStore();
  installCommands();
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("TooltipHost", () => {
  it("shows the tooltip after the pointer rests on the element", async () => {
    renderHost();

    fireEvent.pointerOver(iconOnly());
    await wait(300);
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    await wait(200);

    expect(screen.getByRole("tooltip")).toHaveTextContent("Refresh the list");
  });

  it("shows nothing when the pointer leaves before the delay is over", async () => {
    renderHost();

    fireEvent.pointerOver(iconOnly());
    await wait(200);
    fireEvent.pointerOver(nothing());
    await wait(600);

    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
  });

  describe("a stray title", () => {
    it("is moved to data-tooltip so the native tooltip never shows", () => {
      render(
        <>
          <button type="button" title="Refresh the list" />
          <TooltipHost />
        </>,
      );
      const button = screen.getByRole("button");

      fireEvent.pointerOver(button);

      expect(button).not.toHaveAttribute("title");
      expect(button).toHaveAttribute("data-tooltip", "Refresh the list");
    });

    it("shows as the tooltip after the delay", async () => {
      render(
        <>
          <button type="button" title="Refresh the list" />
          <TooltipHost />
        </>,
      );

      fireEvent.pointerOver(screen.getByRole("button"));
      await wait(450);

      expect(screen.getByRole("tooltip")).toHaveTextContent("Refresh the list");
    });

    it("names an element without text as its aria-label", () => {
      render(
        <>
          <button type="button" title="Refresh the list" />
          <TooltipHost />
        </>,
      );

      fireEvent.pointerOver(screen.getByRole("button"));

      expect(screen.getByRole("button", { name: "Refresh the list" })).toBeInTheDocument();
    });

    it("leaves the name of an element with its own text alone", () => {
      render(
        <>
          <button type="button" title="Sort the rows">
            Sort
          </button>
          <TooltipHost />
        </>,
      );

      fireEvent.pointerOver(screen.getByRole("button", { name: "Sort" }));

      expect(screen.getByRole("button", { name: "Sort" })).not.toHaveAttribute("aria-label");
    });
  });

  it("shows the next element's tooltip at once while one is showing", async () => {
    renderHost();
    fireEvent.pointerOver(iconOnly());
    await wait(450);

    fireEvent.pointerOver(second());

    expect(screen.getByRole("tooltip")).toHaveTextContent("Open the settings");
  });

  it("shows the next element's tooltip at once shortly after one was hidden", async () => {
    renderHost();
    fireEvent.pointerOver(iconOnly());
    await wait(450);
    fireEvent.pointerOver(nothing());
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    await wait(200);

    fireEvent.pointerOver(second());

    expect(screen.getByRole("tooltip")).toHaveTextContent("Open the settings");
  });

  it("waits the full delay again once the tooltips have been gone for a while", async () => {
    renderHost();
    fireEvent.pointerOver(iconOnly());
    await wait(450);
    fireEvent.pointerOver(nothing());
    await wait(1000);

    fireEvent.pointerOver(second());

    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
  });

  describe("keyboard focus", () => {
    beforeEach(() => {
      const matches = Element.prototype.matches;
      vi.spyOn(Element.prototype, "matches").mockImplementation(function (this: Element, selector: string) {
        return selector === ":focus-visible" ? this === document.activeElement : matches.call(this, selector);
      });
    });

    it("shows the tooltip of a focused element after the delay", async () => {
      renderHost();

      act(() => iconOnly().focus());
      await wait(300);
      expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
      await wait(200);

      expect(screen.getByRole("tooltip")).toHaveTextContent("Refresh the list");
    });

    it("hides the tooltip when the element loses focus", async () => {
      renderHost();
      act(() => iconOnly().focus());
      await wait(450);

      act(() => iconOnly().blur());

      expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    });
  });

  describe("near the window's edges", () => {
    const TIP_WIDTH = 100;
    const original = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetWidth");

    beforeEach(() => {
      Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, get: () => TIP_WIDTH });
    });

    afterEach(() => {
      if (original) Object.defineProperty(HTMLElement.prototype, "offsetWidth", original);
      else delete (HTMLElement.prototype as { offsetWidth?: number }).offsetWidth;
    });

    /** Hovers an icon button whose box starts at `left` and is 20px wide; returns the tooltip's left in px. */
    async function tooltipLeftFor(left: number) {
      render(
        <>
          <button type="button" data-tooltip="Refresh the list" />
          <TooltipHost />
        </>,
      );
      const button = screen.getByRole("button");
      button.getBoundingClientRect = () => ({ left, right: left + 20, top: 10, bottom: 30, width: 20, height: 20, x: left, y: 10, toJSON: () => ({}) });
      fireEvent.pointerOver(button);
      await wait(450);
      return parseFloat(screen.getByRole("tooltip").style.left);
    }

    it("keeps a tooltip centered under a control in the middle", async () => {
      expect(await tooltipLeftFor(490)).toBe(500);
    });

    it("moves a tooltip inward so it stays 8px inside the left edge", async () => {
      expect(await tooltipLeftFor(0)).toBe(8 + TIP_WIDTH / 2);
    });

    it("moves a tooltip inward so it stays 8px inside the right edge", async () => {
      expect(await tooltipLeftFor(window.innerWidth - 20)).toBe(window.innerWidth - 8 - TIP_WIDTH / 2);
    });
  });

  describe("text cut off with an ellipsis", () => {
    const TITLE = "A very long issue title that does not fit";

    /** jsdom has no layout: the element reports the widths a browser would give it. */
    function ellipsis(element: HTMLElement, scrollWidth: number, clientWidth: number) {
      element.style.textOverflow = "ellipsis";
      Object.defineProperty(element, "scrollWidth", { configurable: true, value: scrollWidth });
      Object.defineProperty(element, "clientWidth", { configurable: true, value: clientWidth });
      return element;
    }

    const hover = async (element: Element) => {
      fireEvent.pointerOver(element);
      await wait(450);
    };

    it("shows its full text as the tooltip", async () => {
      render(
        <>
          <span>{TITLE}</span>
          <TooltipHost />
        </>,
      );

      await hover(ellipsis(screen.getByText(TITLE), 300, 100));

      expect(screen.getByRole("tooltip")).toHaveTextContent(TITLE);
    });

    it("shows no tooltip when the text fits", async () => {
      render(
        <>
          <span>{TITLE}</span>
          <TooltipHost />
        </>,
      );

      await hover(ellipsis(screen.getByText(TITLE), 100, 100));

      expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    });

    it("shows no tooltip for text that reveals itself on hover", async () => {
      render(
        <>
          <span data-reveals>{TITLE}</span>
          <TooltipHost />
        </>,
      );

      await hover(ellipsis(screen.getByText(TITLE), 300, 100));

      expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    });

    it("takes precedence over a tooltip around it", async () => {
      render(
        <>
          <button type="button" data-tooltip="Open the issue">
            <span>{TITLE}</span>
          </button>
          <TooltipHost />
        </>,
      );

      await hover(ellipsis(screen.getByText(TITLE), 300, 100));

      expect(screen.getByRole("tooltip")).toHaveTextContent(TITLE);
    });

    it("falls back to the tooltip around it when the text fits", async () => {
      render(
        <>
          <button type="button" data-tooltip="Open the issue">
            <span>{TITLE}</span>
          </button>
          <TooltipHost />
        </>,
      );

      await hover(ellipsis(screen.getByText(TITLE), 100, 100));

      expect(screen.getByRole("tooltip")).toHaveTextContent("Open the issue");
    });

    it("shows the cut-off text when the pointer is on an element inside it", async () => {
      render(
        <>
          <span>
            <b>Bold</b> {TITLE}
          </span>
          <TooltipHost />
        </>,
      );
      const bold = screen.getByText("Bold");
      ellipsis(bold.parentElement!, 300, 100);

      await hover(bold);

      expect(screen.getByRole("tooltip")).toHaveTextContent(`Bold ${TITLE}`);
    });

    it("ignores a cut-off text more than three levels above the pointer", async () => {
      render(
        <>
          <div>
            <div>
              <div>
                <div>
                  <i>Deep</i>
                </div>
              </div>
            </div>
          </div>
          <TooltipHost />
        </>,
      );
      const deep = screen.getByText("Deep");
      ellipsis(deep.parentElement!.parentElement!.parentElement!.parentElement!, 300, 100);

      await hover(deep);

      expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    });
  });

  describe("a control bound to a command", () => {
    const hover = async () => {
      fireEvent.pointerOver(screen.getByRole("button", { name: "New tab" }));
      await wait(450);
    };

    beforeEach(() => {
      render(
        <>
          <button type="button" aria-label="New tab" data-tooltip="New query tab" data-command="tabs.new" />
          <button type="button" aria-label="Plain" data-tooltip="Plain tip" />
          <TooltipHost />
        </>,
      );
    });

    it("ends its tooltip with the command's default binding", async () => {
      await hover();

      expect(screen.getByRole("tooltip")).toHaveTextContent("New query tab (⌘T)");
    });

    it("ends its tooltip with a rebinding made since the last tooltip", async () => {
      await hover();
      fireEvent.pointerDown(screen.getByRole("button", { name: "New tab" }));
      await wait(1000);

      act(() => useStore.setState({ keybindings: { "tabs.new": "Mod+Shift+N" } }));
      await hover();

      expect(screen.getByRole("tooltip")).toHaveTextContent("New query tab (⌘⇧N)");
    });

    it("adds no binding when the command is unbound", async () => {
      act(() => useStore.setState({ keybindings: { "tabs.new": null } }));

      await hover();

      expect(screen.getByRole("tooltip")).toHaveTextContent(/^New query tab$/);
    });

    it("adds no binding to a control without a command", async () => {
      fireEvent.pointerOver(screen.getByRole("button", { name: "Plain" }));
      await wait(450);

      expect(screen.getByRole("tooltip")).toHaveTextContent(/^Plain tip$/);
    });
  });

  it.each([
    ["a press", (el: HTMLElement) => fireEvent.pointerDown(el)],
    ["Escape", (el: HTMLElement) => fireEvent.keyDown(el, { key: "Escape" })],
    ["leaving the element", () => fireEvent.pointerOver(nothing())],
    ["scrolling", (el: HTMLElement) => fireEvent.wheel(el)],
  ])("hides the tooltip on %s", async (_name, hide) => {
    renderHost();
    fireEvent.pointerOver(iconOnly());
    await wait(450);
    expect(screen.getByRole("tooltip")).toBeInTheDocument();

    hide(iconOnly());

    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
  });
});
