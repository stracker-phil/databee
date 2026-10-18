import { render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));
vi.mock("./api", async () => ({ stageApi: (await import("./apiMock")).createStageApiMock() }));

import { api } from "../api";
import { installCommands } from "../commands";
import { projectKey } from "../state/slices/project";
import { resetStore, useStore } from "../state/store";
import { connectedStore } from "../test/fixtures";
import { activeObject, objectDocument } from "../test/objectFixtures";
import { registerCoreSettingsPages } from "../components/settings/pages";
import { stageApi } from "./api";
import { installStage } from "./install";
import type { StageAct, StageActRequest, StageKeys } from "./types";

const mocked = vi.mocked(stageApi, true);
const mocked_api = vi.mocked(api, true);

/** jsdom has no layout: elements take their box from a `data-rect="x,y,width,height"` attribute. */
function stubLayout() {
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    const [x, y, width, height] = (this.getAttribute("data-rect") ?? "0,0,0,0").split(",").map(Number);
    return { x, y, width, height, top: y, left: x, right: x + width, bottom: y + height, toJSON: () => ({}) };
  });
  document.elementFromPoint = () => null;
  Element.prototype.scrollIntoView = vi.fn();
}

const key = (k: string, code: string, mods: Partial<StageKeys> = {}): StageKeys => ({ key: k, code, meta: false, shift: false, alt: false, ctrl: false, ...mods });

/** Installs the hooks and returns what Rust would send them. */
function hooks() {
  installStage();
  const handler = <T,>(on: { mock: { calls: unknown[][] } }) => on.mock.calls.at(-1)![0] as (arg: T) => void;
  const act = async (request: StageAct) => {
    mocked.done.mockClear();
    handler<StageActRequest>(mocked.onAct)({ request: "r1", ...request });
    await vi.waitFor(() => expect(mocked.done).toHaveBeenCalled());
    return mocked.done.mock.calls.at(-1)![0];
  };
  return {
    act,
    collect: () => handler<string>(mocked.onCollect)("c1"),
    info: () => {
      handler<string>(mocked.onInfo)("i1");
      return mocked.info.mock.calls.at(-1)![0];
    },
  };
}

describe("Stage page hooks", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetStore();
    registerCoreSettingsPages();
    stubLayout();
    vi.spyOn(document, "hasFocus").mockReturnValue(true);
  });
  afterEach(() => vi.restoreAllMocks());

  it("reports each sized anchor once, with its window, role, list and states", () => {
    render(
      <div>
        <button data-stage="toolbar:settings" aria-label="Settings" data-rect="10,8,24,24" />
        <button data-stage="toolbar:settings" aria-label="Duplicate" data-rect="50,8,24,24" />
        <button data-stage="toolbar:sidebar" aria-pressed="true" data-rect="80,8,60,24">Sidebar</button>
        <div data-stage="table:db.wp_posts" data-stage-list="tables" aria-selected="true" data-rect="0,2000,240,24">
          <span data-stage-label="wp_posts">wp_posts 12k</span>
        </div>
        <input data-stage="sidebar:filter" aria-label="Filter tables" data-rect="0,40,200,24" disabled />
        <div data-stage="hidden" data-rect="0,0,0,0">gone</div>
        <div role="dialog" data-stage-window="settings" data-rect="200,100,700,600">
          <button data-stage="settings-nav:queries" data-stage-list="settings-nav" aria-current="page" data-rect="210,140,150,26">Queries</button>
        </div>
      </div>,
    );
    hooks().collect();
    expect(mocked.anchors).toHaveBeenCalledWith("c1", "main", [
      { id: "toolbar:settings", label: "Settings", window: "main", rect: { x: 10, y: 8, width: 24, height: 24 }, inView: true, visible: { x: 10, y: 8, width: 24, height: 24 }, role: "button" },
      { id: "toolbar:sidebar", label: "Sidebar", window: "main", rect: { x: 80, y: 8, width: 60, height: 24 }, inView: true, visible: { x: 80, y: 8, width: 60, height: 24 }, role: "toggle", checked: true },
      { id: "table:db.wp_posts", label: "wp_posts", window: "main", rect: { x: 0, y: 2000, width: 240, height: 24 }, inView: false, visible: null, role: "item", list: "tables", selected: true },
      { id: "sidebar:filter", label: "Filter tables", window: "main", rect: { x: 0, y: 40, width: 200, height: 24 }, inView: true, visible: { x: 0, y: 40, width: 200, height: 24 }, role: "field", disabled: true },
      { id: "settings-nav:queries", label: "Queries", window: "settings", rect: { x: 210, y: 140, width: 150, height: 26 }, inView: true, visible: { x: 210, y: 140, width: 150, height: 26 }, role: "item", list: "settings-nav", selected: true },
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
    expect(reply).toEqual({
      request: "r1",
      page: "main",
      handled: true,
      ok: true,
      window: "main",
      rect: { x: 100, y: 700, width: 80, height: 28 },
      visible: { x: 100, y: 700, width: 80, height: 28 },
    });
  });

  it("refuses disabled controls and leaves unknown anchors unhandled", async () => {
    render(<button data-stage="query:run" data-rect="0,0,60,24" disabled>Run</button>);
    const { act } = hooks();
    expect(await act({ kind: "click", anchor: "query:run" })).toMatchObject({ handled: true, ok: false, error: "“query:run” is disabled" });
    expect(await act({ kind: "reveal", anchor: "nope" })).toMatchObject({ handled: false, ok: false });
  });

  it("replaces or appends to a field's text so React sees the change", async () => {
    function Filter() {
      const [value, setValue] = useState("");
      return (
        <>
          <input data-stage="palette:search" data-rect="0,0,200,24" aria-label="Search" value={value} onChange={(e) => setValue(e.target.value)} />
          <output>{value}</output>
        </>
      );
    }
    render(<Filter />);
    const { act } = hooks();
    expect(await act({ kind: "type", anchor: "palette:search", text: "Hel", mode: "replace" })).toMatchObject({ ok: true });
    await act({ kind: "type", anchor: "palette:search", text: "lo", mode: "append" });
    expect(screen.getByRole("status")).toHaveTextContent("Hello");
    await act({ kind: "type", anchor: "palette:search", text: "wp_options", mode: "replace" });
    expect(screen.getByRole("status")).toHaveTextContent(/^wp_options$/);
    expect(screen.getByLabelText("Search")).toHaveFocus();
  });

  it("fails to type into something that isn't a field", async () => {
    render(<button data-stage="toolbar:settings" data-rect="0,0,24,24">S</button>);
    expect(await hooks().act({ kind: "type", anchor: "toolbar:settings", text: "x", mode: "replace" })).toMatchObject({ ok: false, error: "“toolbar:settings” isn't a text field" });
  });

  it("scrolls the scroll area at the anchor and refuses one that can't scroll", async () => {
    render(
      <>
        <div data-stage="results" data-rect="0,0,400,300" style={{ overflowY: "auto" }} />
        <div data-stage="statusbar" data-rect="0,300,400,20" />
      </>,
    );
    const area = document.querySelector<HTMLElement>('[data-stage="results"]')!;
    Object.defineProperty(area, "scrollHeight", { value: 2000 });
    Object.defineProperty(area, "clientHeight", { value: 300 });
    area.scrollBy = vi.fn() as typeof area.scrollBy;
    area.scrollTo = vi.fn() as typeof area.scrollTo;
    const { act } = hooks();
    expect(await act({ kind: "scroll", anchor: "results", by: 120 })).toMatchObject({ ok: true, window: "main" });
    expect(area.scrollBy).toHaveBeenCalledWith({ top: 120, behavior: "smooth" });
    await act({ kind: "scroll", anchor: "results", to: "bottom" });
    expect(area.scrollTo).toHaveBeenCalledWith({ top: 2000, behavior: "smooth" });
    expect(await act({ kind: "scroll", anchor: "statusbar", by: 10 })).toMatchObject({ ok: false, status: 409, error: "“statusbar” isn't scrollable" });
  });

  it("runs a command's shortcut even without focus, as its menu accelerator would", async () => {
    connectedStore();
    installCommands();
    vi.mocked(document.hasFocus).mockReturnValue(false);
    const reply = await hooks().act({ kind: "keys", keys: key(",", "", { meta: true }), focused: false });
    expect(reply).toMatchObject({ handled: true, ok: true, command: "app:settings" });
    expect(useStore.getState().dialog).toEqual({ type: "settings" });
  });

  it("sends other keys to the focused element, and Enter submits its form", async () => {
    const onKey = vi.fn();
    const onSubmit = vi.fn((e: React.FormEvent) => e.preventDefault());
    render(
      <form onSubmit={onSubmit}>
        <input aria-label="WHERE filter" onKeyDown={(e) => onKey(e.key)} autoFocus />
      </form>,
    );
    const { act } = hooks();
    expect(await act({ kind: "keys", keys: key("ArrowDown", "ArrowDown"), focused: false })).toMatchObject({ handled: true, ok: true, window: "main" });
    expect(onKey).toHaveBeenLastCalledWith("ArrowDown");
    await act({ kind: "keys", keys: key("Enter", "Enter"), focused: false });
    expect(onSubmit).toHaveBeenCalledTimes(1);

    vi.mocked(document.hasFocus).mockReturnValue(false);
    expect(await act({ kind: "keys", keys: key("Escape", "Escape"), focused: false })).toMatchObject({ handled: false });
    // Rust focused the window for this request (keys with window=…).
    expect(await act({ kind: "keys", keys: key("Escape", "Escape"), focused: true })).toMatchObject({ handled: true, ok: true });
    expect(onKey).toHaveBeenLastCalledWith("Escape");
  });

  it("runs commands by their Stage ids, including families", async () => {
    connectedStore();
    installCommands();
    const { act } = hooks();
    expect(await act({ kind: "command", command: "settings:open:queries" })).toMatchObject({ ok: true });
    expect(useStore.getState().dialog).toEqual({ type: "settings", page: "queries" });
    expect(await act({ kind: "command", command: "table:open:wp_options" })).toMatchObject({ ok: true });
    expect(useStore.getState().tabs.at(-1)).toMatchObject({ kind: "table", table: "wp_options" });
    expect(await act({ kind: "command", command: "view:palette" })).toMatchObject({ ok: true });
    expect(useStore.getState().paletteOpen).toBe(true);
    expect(await act({ kind: "command", command: "settings:open:nope" })).toMatchObject({ ok: false, status: 404 });
    expect(await act({ kind: "command", command: "changes.apply" })).toMatchObject({ ok: false, error: "“changes.apply” can't run in the current view" });
  });

  describe("entity:open: family", () => {
    beforeEach(() => {
      connectedStore();
      installCommands();
      mocked_api.document.mockResolvedValue(objectDocument());
    });

    it("opens the row the entity and id name", async () => {
      useStore.setState({ rulesInfo: { [projectKey("s1", "db")]: { prefix: "wp_", networkPrefix: "wp_", suggestedPrefix: null, appliedPresets: [], availablePresets: [], entities: [{ preset: "woocommerce", entity: { name: "order", table: "wp_wc_orders" } }], tableEntities: {}, tableIcons: {} } } });
      expect(await hooks().act({ kind: "command", command: "entity:open:order:4711" })).toMatchObject({ ok: true });
      await vi.waitFor(() => expect(activeObject().title).toBe("wp_wc_orders #4711"));
      expect(mocked_api.document).toHaveBeenCalledWith("s1", "order 4711", "db");
    });

    it("cannot run when the rules name no entity", async () => {
      expect(await hooks().act({ kind: "command", command: "entity:open:order:4711" })).toMatchObject({ ok: false, status: 409 });
      expect(useStore.getState().tabs.some((t) => t.kind === "object")).toBe(false);
    });
  });

  it("opens and closes the settings and palette sheets", async () => {
    connectedStore();
    const { act } = hooks();
    await act({ kind: "window", window: "settings", action: "show", section: "presentations" });
    expect(useStore.getState().dialog).toEqual({ type: "settings", page: "presentations" });
    await act({ kind: "window", window: "settings", action: "hide" });
    expect(useStore.getState().dialog).toBeNull();
    await act({ kind: "window", window: "palette", action: "show" });
    expect(useStore.getState().paletteOpen).toBe(true);
    await act({ kind: "window", window: "palette", action: "hide" });
    expect(useStore.getState().paletteOpen).toBe(false);
  });

  describe("stage:act replies", () => {
    it("replies to a window request after opening the sheet", async () => {
      connectedStore();
      expect(await hooks().act({ kind: "window", window: "palette", action: "show" })).toEqual({ request: "r1", page: "main", handled: true, ok: true });
      expect(useStore.getState().paletteOpen).toBe(true);
    });

    it("replies to a command request with the command's failure", async () => {
      connectedStore();
      installCommands();
      const { act } = hooks();
      expect(await act({ kind: "command", command: "view:palette" })).toMatchObject({ handled: true, ok: true });
      expect(useStore.getState().paletteOpen).toBe(true);
      expect(await act({ kind: "command", command: "nope" })).toMatchObject({ handled: true, ok: false, status: 404 });
    });

    it("replies to a keys request with the window that had the keys", async () => {
      const onKey = vi.fn();
      render(<input aria-label="Field" onKeyDown={(e) => onKey(e.key)} autoFocus />);
      expect(await hooks().act({ kind: "keys", keys: key("a", "KeyA"), focused: false })).toMatchObject({ handled: true, ok: true, window: "main" });
      expect(onKey).toHaveBeenCalledWith("a");
    });

    it("clicks the anchor and replies with its rect", async () => {
      const onClick = vi.fn();
      render(<button data-stage="tray:apply" data-rect="1,2,30,40" onClick={onClick}>Apply</button>);
      expect(await hooks().act({ kind: "click", anchor: "tray:apply" })).toMatchObject({ handled: true, ok: true, rect: { x: 1, y: 2, width: 30, height: 40 } });
      expect(onClick).toHaveBeenCalled();
    });

    it("appends typed text to the field's current value in append mode", async () => {
      render(<input data-stage="palette:search" data-rect="0,0,200,24" aria-label="Search" defaultValue="wp_" />);
      expect(await hooks().act({ kind: "type", anchor: "palette:search", text: "posts", mode: "append" })).toMatchObject({ handled: true, ok: true });
      expect(screen.getByLabelText("Search")).toHaveValue("wp_posts");
    });

    it("reveals an anchor without acting on it", async () => {
      const onClick = vi.fn();
      render(<button data-stage="tray:apply" data-rect="1,2,30,40" onClick={onClick}>Apply</button>);
      expect(await hooks().act({ kind: "reveal", anchor: "tray:apply" })).toMatchObject({ handled: true, ok: true, window: "main" });
      expect(onClick).not.toHaveBeenCalled();
    });

    it("scrolls the anchor's scroll area and replies done", async () => {
      render(<div data-stage="results" data-rect="0,0,400,300" style={{ overflowY: "auto" }} />);
      const area = document.querySelector<HTMLElement>("[data-stage]")!;
      Object.defineProperty(area, "scrollHeight", { value: 2000 });
      Object.defineProperty(area, "clientHeight", { value: 300 });
      area.scrollTo = vi.fn() as typeof area.scrollTo;
      expect(await hooks().act({ kind: "scroll", anchor: "results", to: "top" })).toMatchObject({ handled: true, ok: true });
      expect(area.scrollTo).toHaveBeenCalledWith({ top: 0, behavior: "smooth" });
    });
  });

  it("describes the view, panels, sheets and commands with shortcuts and families", () => {
    connectedStore();
    installCommands();
    useStore.getState().openDialog({ type: "settings", page: "shortcuts" });
    render(<div role="dialog" data-stage-window="settings" data-rect="200,100,700,600" />);
    const reply = hooks().info();
    expect(reply).toMatchObject({ request: "i1", page: "main", view: "db", ui: { valueColumn: "closed" } });
    expect(reply.state).toMatchObject({ connection: "root@127.0.0.1:52958/db", tab: "query: Query 1", settings: "shortcuts", palette: "closed" });
    const settings = reply.surfaces.find((s) => s.id === "settings")!;
    expect(settings).toMatchObject({ title: "Settings", rect: { x: 200, y: 100, width: 700, height: 600 }, section: "shortcuts" });
    expect(settings.sections).toContainEqual({ id: "presentations", title: "Presentations" });
    expect(reply.surfaces.find((s) => s.id === "palette")).toMatchObject({ rect: null });
    expect(reply.commands).toContainEqual({ id: "app:settings", help: expect.any(String), shortcut: "cmd+,", enabled: true });
    expect(reply.commands).toContainEqual(expect.objectContaining({ id: "editor.runAll", shortcut: "shift+cmd+enter" }));
    expect(reply.commands.find((c) => c.id === "changes.apply")?.enabled).toBe(false);
    const tables = reply.commands.find((c) => c.id === "table:open:")!;
    expect(tables).toMatchObject({ arg: "<table>", enabled: true });
    expect(tables.choices).toContainEqual({ id: "table:open:wp_options", title: "wp_options" });
  });
});
