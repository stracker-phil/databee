import { render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api, type StageActRequest } from "../api";
import { installCommands } from "../commands";
import { resetStore } from "../state/store";
import { connectedStore } from "../test/fixtures";
import { installStageHooks } from "./hooks";

const mocked = vi.mocked(api, true);

/** jsdom has no layout: elements take their box from a `data-rect="x,y,width,height"` attribute. */
function stubLayout() {
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    const [x, y, width, height] = (this.getAttribute("data-rect") ?? "0,0,0,0").split(",").map(Number);
    return { x, y, width, height, top: y, left: x, right: x + width, bottom: y + height, toJSON: () => ({}) } as DOMRect;
  });
  document.elementFromPoint = () => null;
  Element.prototype.scrollIntoView = vi.fn();
}

/** Installs the hooks and returns what Rust would send them. */
function hooks(run: (id: string) => boolean = vi.fn(() => true)) {
  installStageHooks(run);
  const handler = <T,>(on: { mock: { calls: unknown[][] } }) => on.mock.calls.at(-1)![0] as (arg: T) => void;
  const act = async (request: Omit<StageActRequest, "request">) => {
    handler<StageActRequest>(mocked.onStageAct)({ request: "r1", ...request });
    await vi.waitFor(() => expect(mocked.stageDone).toHaveBeenCalled());
    return mocked.stageDone.mock.calls.at(-1)![0];
  };
  return {
    run,
    act,
    collect: () => handler<string>(mocked.onStageCollect)("c1"),
    info: () => handler<string>(mocked.onStageInfo)("i1"),
  };
}

describe("Stage page hooks", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetStore();
    stubLayout();
    vi.spyOn(document, "hasFocus").mockReturnValue(true);
  });
  afterEach(() => vi.restoreAllMocks());

  it("reports each sized anchor once, with its label and whether it shows", () => {
    render(
      <div>
        <button data-stage="toolbar:settings" aria-label="Settings" data-rect="10,8,24,24" />
        <button data-stage="toolbar:settings" aria-label="Duplicate" data-rect="50,8,24,24" />
        <div data-stage="sidebar" data-stage-label="Sidebar" data-rect="0,40,240,700">tables…</div>
        <div data-stage="table:db.wp_posts" data-rect="0,2000,240,24">
          <span data-stage-label="wp_posts">wp_posts 12k</span>
        </div>
        <div data-stage="hidden" data-rect="0,0,0,0">gone</div>
      </div>,
    );
    hooks().collect();
    expect(mocked.stageAnchors).toHaveBeenCalledWith("c1", [
      { id: "toolbar:settings", label: "Settings", rect: { x: 10, y: 8, width: 24, height: 24 }, inView: true },
      { id: "sidebar", label: "Sidebar", rect: { x: 0, y: 40, width: 240, height: 700 }, inView: true },
      { id: "table:db.wp_posts", label: "wp_posts", rect: { x: 0, y: 2000, width: 240, height: 24 }, inView: false },
    ]);
  });

  it("clicks the button inside a wrapper anchor and reports its rect", async () => {
    const onClick = vi.fn();
    render(
      <div data-stage="tray:apply" data-rect="100,700,80,28">
        <button onClick={onClick}>Apply</button>
      </div>,
    );
    const reply = await hooks().act({ kind: "click", anchor: "tray:apply" });
    expect(onClick).toHaveBeenCalledTimes(1);
    expect(reply).toEqual({ request: "r1", page: "main", handled: true, ok: true, rect: { x: 100, y: 700, width: 80, height: 28 } });
  });

  it("refuses disabled controls and leaves unknown anchors unhandled", async () => {
    render(<button data-stage="query:run" data-rect="0,0,60,24" disabled>Run</button>);
    const { act } = hooks();
    expect(await act({ kind: "click", anchor: "query:run" })).toMatchObject({ handled: true, ok: false, error: "“query:run” is disabled" });
    expect(await act({ kind: "reveal", anchor: "nope" })).toMatchObject({ handled: false, ok: false });
  });

  it("types into fields so React sees the change", async () => {
    function Filter() {
      const [value, setValue] = useState("");
      return (
        <>
          <input data-stage="sidebar:filter" data-rect="0,0,200,24" aria-label="Filter" value={value} onChange={(e) => setValue(e.target.value)} />
          <output>{value}</output>
        </>
      );
    }
    render(<Filter />);
    const reply = await hooks().act({ kind: "type", anchor: "sidebar:filter", text: "wp_options" });
    expect(reply).toMatchObject({ ok: true });
    expect(screen.getByRole("status")).toHaveTextContent("wp_options");
    expect(screen.getByLabelText("Filter")).toHaveFocus();
  });

  it("fails to type into something that isn't a field", async () => {
    render(<button data-stage="toolbar:settings" data-rect="0,0,24,24">S</button>);
    expect(await hooks().act({ kind: "type", anchor: "toolbar:settings", text: "x" })).toMatchObject({ ok: false, error: "“toolbar:settings” isn't a text field" });
  });

  it("sends keys to the focused element, and Enter submits its form", async () => {
    const onKey = vi.fn();
    const onSubmit = vi.fn((e: React.FormEvent) => e.preventDefault());
    render(
      <form onSubmit={onSubmit}>
        <input aria-label="WHERE filter" onKeyDown={(e) => onKey(`${e.metaKey ? "⌘" : ""}${e.key}`)} autoFocus />
      </form>,
    );
    const { act } = hooks();
    expect(await act({ kind: "keys", keys: { key: "k", code: "KeyK", meta: true, shift: false, alt: false, ctrl: false } })).toMatchObject({ handled: true, ok: true });
    expect(onKey).toHaveBeenLastCalledWith("⌘k");
    expect(onSubmit).not.toHaveBeenCalled();
    await act({ kind: "keys", keys: { key: "Enter", code: "Enter", meta: false, shift: false, alt: false, ctrl: false } });
    expect(onSubmit).toHaveBeenCalledTimes(1);
  });

  it("leaves keys unhandled while the window has no focus", async () => {
    vi.mocked(document.hasFocus).mockReturnValue(false);
    expect(await hooks().act({ kind: "keys", keys: { key: "Escape", code: "Escape", meta: false, shift: false, alt: false, ctrl: false } })).toMatchObject({ handled: false });
  });

  it("runs commands and says when one isn't available", async () => {
    const run = vi.fn((id: string): boolean => id === "settings.open");
    const { act } = hooks(run);
    expect(await act({ kind: "command", command: "settings.open" })).toMatchObject({ handled: true, ok: true });
    expect(run).toHaveBeenCalledWith("settings.open");
    expect(await act({ kind: "command", command: "editor.run" })).toMatchObject({ ok: false, error: "“editor.run” isn't available now" });
  });

  it("describes what the app shows and which commands run now", () => {
    connectedStore();
    installCommands();
    hooks().info();
    const reply = mocked.stageInfo.mock.calls[0][0];
    expect(reply).toMatchObject({ request: "i1", page: "main", view: "db" });
    expect(reply.state).toMatchObject({ mode: "db", connection: "root@127.0.0.1:52958/db", database: "db", tab: "query: Query 1", pendingChanges: 0, inspector: false });
    expect(reply.commands).toContainEqual({ id: "settings.open", help: expect.any(String), available: true });
    expect(reply.commands.find((c) => c.id === "changes.apply")?.available).toBe(false);
  });
});
