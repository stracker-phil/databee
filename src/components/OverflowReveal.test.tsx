import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { useOverflowReveal } from "./OverflowReveal";

function Harness() {
  const [lit, setLit] = useState(false);
  const { onMouseOver, onMouseLeave, element } = useOverflowReveal(".row", ".label");
  return (
    <div onMouseOver={onMouseOver} onMouseLeave={onMouseLeave}>
      <style>{".label { color: rgb(1, 2, 3); } .row.lit .label { color: rgb(200, 100, 50); }"}</style>
      <div className={lit ? "row lit" : "row"} onMouseOver={() => setLit(true)}>
        <span className="label">a very long table name</span>
      </div>
      <button>elsewhere</button>
      {element}
    </div>
  );
}

const overlay = () => document.querySelector<HTMLElement>(".overflow-reveal");

describe("useOverflowReveal", () => {
  afterEach(() => {
    Object.defineProperty(HTMLElement.prototype, "scrollWidth", { configurable: true, get: () => 0 });
  });

  const overflow = (on: boolean) => {
    Object.defineProperty(HTMLElement.prototype, "scrollWidth", { configurable: true, get: () => (on ? 200 : 0) });
    Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => (on ? 100 : 0) });
  };

  it("shows the whole label text in an overlay while a cut-off row is hovered", async () => {
    overflow(true);
    render(<Harness />);
    await userEvent.hover(screen.getByText("a very long table name"));
    expect(overlay()).toHaveTextContent("a very long table name");
  });

  it("draws the overlay in the color the same hover lights the row with", async () => {
    overflow(true);
    render(<Harness />);
    await userEvent.hover(screen.getByText("a very long table name"));
    expect(overlay()?.style.color).toBe("rgb(200, 100, 50)");
  });

  it("shows no overlay when the label is not cut off", async () => {
    overflow(false);
    render(<Harness />);
    await userEvent.hover(screen.getByText("a very long table name"));
    expect(overlay()).toBeNull();
  });

  it("removes the overlay when the pointer leaves", async () => {
    overflow(true);
    render(<Harness />);
    const label = screen.getByText("a very long table name");
    await userEvent.hover(label);
    await userEvent.unhover(label);
    expect(overlay()).toBeNull();
  });
});
