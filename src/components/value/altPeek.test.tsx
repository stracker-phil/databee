import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useAltPeek } from "./altPeek";

function Probe() {
  const { held, target } = useAltPeek();
  return <output>{held ? `held on ${target?.textContent ?? "nothing"}` : "released"}</output>;
}

function renderCells() {
  render(
    <>
      <Probe />
      <p>plain</p>
      <span data-peek="">stamp one</span>
      <span data-peek="">stamp two</span>
      <span data-ref="databee://shop/db/wp_users/12">reference</span>
      <div data-db-change="">
        changed <span data-ref="databee://shop/db/wp_users/12">inner reference</span>
      </div>
    </>,
  );
}

const status = () => screen.getByRole("status");
const over = (text: string | RegExp) => fireEvent.mouseOver(screen.getByText(text));
const press = (init: KeyboardEventInit = {}) => fireEvent.keyDown(window, { key: "Alt", altKey: true, ...init });
const release = () => fireEvent.keyUp(window, { key: "Alt" });

describe("useAltPeek", () => {
  it("is released until Alt is pressed", () => {
    renderCells();
    over("stamp one");
    expect(status()).toHaveTextContent("released");
  });

  it.each([
    ["a data-peek element", "stamp one", "held on stamp one"],
    ["a data-ref element", "reference", "held on reference"],
    ["a changed cell, over the reference inside it", "inner reference", /^held on changed inner reference/],
  ])("peeks %s under the pointer when Alt is pressed", (_name, hovered, expected) => {
    renderCells();
    over(hovered);
    press();
    expect(status()).toHaveTextContent(expected);
  });

  it("holds without a target when nothing peekable is under the pointer", () => {
    renderCells();
    over("plain");
    press();
    expect(status()).toHaveTextContent("held on nothing");
  });

  it("peeks the element the pointer arrives on after Alt was pressed", () => {
    renderCells();
    press();
    over("stamp two");
    expect(status()).toHaveTextContent("held on stamp two");
  });

  it("switches to another peekable element the pointer moves onto", () => {
    renderCells();
    over("stamp one");
    press();
    over("reference");
    expect(status()).toHaveTextContent("held on reference");
  });

  it("keeps the peek when the pointer moves onto something that cannot be peeked", () => {
    renderCells();
    over("stamp one");
    press();
    over("plain");
    expect(status()).toHaveTextContent("held on stamp one");
  });

  it("ends the peek when Alt is released", () => {
    renderCells();
    over("stamp one");
    press();
    release();
    expect(status()).toHaveTextContent("released");
  });

  it("ends the peek when the window loses focus", () => {
    renderCells();
    over("stamp one");
    press();
    fireEvent.blur(window);
    expect(status()).toHaveTextContent("released");
  });

  it.each([
    ["Meta", { metaKey: true }],
    ["Ctrl", { ctrlKey: true }],
    ["Shift", { shiftKey: true }],
  ])("ignores Alt pressed together with %s", (_name, modifier) => {
    renderCells();
    over("stamp one");
    press(modifier);
    expect(status()).toHaveTextContent("released");
  });

  it("ignores other keys", () => {
    renderCells();
    over("stamp one");
    fireEvent.keyDown(window, { key: "a" });
    expect(status()).toHaveTextContent("released");
  });
});
