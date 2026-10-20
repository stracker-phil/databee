import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import menuSource from "../../src-tauri/src/menu.rs?raw";
import { allCommands, bindingFor, clearCommands, getCommand, installCommands } from "./index";

interface MenuItem {
  id: string;
  binding: string;
  bindingOf: string | null;
}

function menuItems(source: string): MenuItem[] {
  const bindingOf = new Map<string, string>();
  for (const m of source.matchAll(/binding_of:\s*Some\("([^"]+)"\),\s*\.\.item\(\s*"([^"]+)"/g)) bindingOf.set(m[2], m[1]);
  return [...source.matchAll(/item\(\s*"([^"]+)",\s*"[^"]*",\s*"([^"]*)",?\s*\)/g)].map((m) => ({ id: m[1], binding: m[2], bindingOf: bindingOf.get(m[1]) ?? null }));
}

describe("native menu items in menu.rs", () => {
  const items = menuItems(menuSource);
  const ids = new Set(items.map((i) => i.id));

  beforeAll(() => installCommands());
  afterAll(() => clearCommands());

  it("lists the items", () => {
    expect(items.length).toBeGreaterThan(40);
    expect(items.filter((i) => i.bindingOf)).toEqual([{ id: "query.save", binding: "", bindingOf: "changes.apply" }]);
  });

  it.each(menuItems(menuSource).filter((i) => !i.bindingOf))("matches the command $id and its default binding", ({ id, binding }) => {
    expect(getCommand(id)?.menu).toBe(true);
    expect(bindingFor(id, {})).toBe(binding === "" ? null : binding);
  });

  it.each(menuItems(menuSource).filter((i) => i.bindingOf))("takes the binding of $bindingOf for $id", ({ id, bindingOf }) => {
    expect(getCommand(id)?.menu).toBe(true);
    expect(getCommand(id)?.fallbackFor).toBe(bindingOf);
  });

  it("has an item for every command that declares a menu item", () => {
    const missing = allCommands().filter((c) => c.menu === true && !ids.has(c.id)).map((c) => c.id);
    expect(missing).toEqual([]);
  });

  it("has the shared item for every command that points at another command's item", () => {
    const dangling = allCommands().filter((c) => typeof c.menu === "string" && !ids.has(c.menu)).map((c) => c.id);
    expect(dangling).toEqual([]);
  });
});
