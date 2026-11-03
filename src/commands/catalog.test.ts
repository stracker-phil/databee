import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api } from "../api";
import { CATALOG, catalogCommands } from "./catalog";
import { allCommands, clearCommands, getCommand, installCommands, runCommand } from "./index";

const mocked = vi.mocked(api, true);

const pageIds = CATALOG.filter((info) => info.runs === "page").map((info) => info.id);
const pageHandlers = () => pageIds.map((id) => ({ id, run: vi.fn() }));

describe("catalogCommands", () => {
  beforeEach(() => vi.clearAllMocks());

  it("throws for a handler whose id is not in the catalog", () => {
    expect(() => catalogCommands([...pageHandlers(), { id: "no.such.command", run: vi.fn() }])).toThrow('Command "no.such.command" is missing from the catalog');
  });

  it("throws for a page command without a handler", () => {
    expect(() => catalogCommands(pageHandlers().slice(1))).toThrow(`Page command "${pageIds[0]}" has no handler`);
  });

  it("throws for a handler given to a command that runs in Rust", () => {
    expect(() => catalogCommands([...pageHandlers(), { id: "view.zoomIn", run: vi.fn() }])).toThrow('Command "view.zoomIn" runs in Rust; remove its page handler');
  });

  it("runs a Rust command through the api", async () => {
    const zoomIn = catalogCommands(pageHandlers()).find((command) => command.id === "view.zoomIn")!;
    await zoomIn.run();
    expect(mocked.runCommand).toHaveBeenCalledWith("view.zoomIn");
  });

  it("runs a page command through its handler instead of the api", async () => {
    const handlers = pageHandlers();
    await catalogCommands(handlers).find((command) => command.id === "tabs.close")!.run();
    expect(handlers.find((handler) => handler.id === "tabs.close")!.run).toHaveBeenCalled();
    expect(mocked.runCommand).not.toHaveBeenCalled();
  });

  describe("metadata from the catalog", () => {
    const command = (id: string) => catalogCommands(pageHandlers()).find((c) => c.id === id)!;

    it("takes title, binding and a menu item from a command with a menu label", () => {
      expect(command("tabs.close")).toMatchObject({ title: "Close tab", defaultBinding: "Mod+W", menu: true });
    });

    it("points a command without a menu label at the item it shares", () => {
      expect(command("object.copyRaw").menu).toBe("value.copyRaw");
    });

    it("carries the command it is a fallback for", () => {
      expect(command("query.save").fallbackFor).toBe("changes.apply");
    });

    it("carries skipInText", () => {
      expect(command("row.delete").skipInText).toBe(true);
    });
  });
});

describe("installCommands", () => {
  beforeEach(() => vi.clearAllMocks());
  afterEach(() => clearCommands());

  it("registers every command of the catalog", () => {
    installCommands();
    expect(allCommands().map((command) => command.id).sort()).toEqual(CATALOG.map((info) => info.id).sort());
  });

  it("runs a zoom command in Rust", () => {
    installCommands();
    expect(runCommand("view.zoomIn")).toBe(true);
    expect(mocked.runCommand).toHaveBeenCalledWith("view.zoomIn");
    expect(getCommand("view.zoomIn")?.title).toBe("Zoom in");
  });
});
