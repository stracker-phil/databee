import { afterEach, describe, expect, it, vi } from "vitest";
import {
  bindingFor,
  bindingFromEvent,
  bindingLabel,
  clearCommands,
  conflicts,
  normalizeBinding,
  registerCommands,
  resolveKeydown,
  setOverrides,
  type Command,
} from "./registry";

const key = (key: string, mods: Partial<{ meta: boolean; ctrl: boolean; alt: boolean; shift: boolean }> = {}, target: EventTarget | null = null) => ({
  key,
  metaKey: mods.meta ?? false,
  ctrlKey: mods.ctrl ?? false,
  altKey: mods.alt ?? false,
  shiftKey: mods.shift ?? false,
  target,
});

function cmd(id: string, scope: Command["scope"], defaultBinding?: string, extra: Partial<Command> = {}): Command {
  return { id, title: id, scope, defaultBinding, run: vi.fn(), ...extra };
}

describe("bindings", () => {
  it("normalizes modifier order and key case", () => {
    expect(normalizeBinding("shift+mod+k")).toBe("Mod+Shift+K");
    expect(normalizeBinding("Cmd+[")).toBe("Mod+[");
    expect(normalizeBinding("Ctrl+Tab")).toBe("Ctrl+Tab");
    expect(normalizeBinding("Mod+")).toBeNull();
  });

  it("treats a trailing ++ as the plus key", () => {
    expect(normalizeBinding("Mod++")).toBe("Mod++");
    expect(normalizeBinding("shift+mod++")).toBe("Mod+Shift++");
    expect(bindingLabel("Mod+Shift++")).toBe("⌘⇧+");
  });

  it("returns an override that binds the plus key", () => {
    registerCommands([cmd("x", "global")]);
    setOverrides({ x: "Mod+Shift++" });
    expect(bindingFor("x")).toBe("Mod+Shift++");
    clearCommands();
  });

  it("reads bindings from keyboard events and labels them with macOS glyphs", () => {
    expect(bindingFromEvent(key("k", { meta: true, shift: true }))).toBe("Mod+Shift+K");
    expect(bindingFromEvent(key("Tab", { ctrl: true }))).toBe("Ctrl+Tab");
    expect(bindingFromEvent(key("Meta", { meta: true }))).toBeNull();
    expect(bindingLabel("Mod+Shift+K")).toBe("⌘⇧K");
    expect(bindingLabel("Ctrl+Tab")).toBe("⌃⇥");
    expect(bindingLabel("Mod+Enter")).toBe("⌘↵");
  });
});

describe("resolveKeydown", () => {
  afterEach(clearCommands);

  it("prefers the most specific active scope", () => {
    const global = cmd("g", "global", "Mod+F");
    const table = cmd("t", "table", "Mod+F");
    registerCommands([global, table]);
    expect(resolveKeydown(key("f", { meta: true }), ["global", "db", "table"])?.id).toBe("t");
    expect(resolveKeydown(key("f", { meta: true }), ["global", "wp"])?.id).toBe("g");
  });

  it("skips disabled commands and inactive scopes", () => {
    const off = cmd("off", "db", "Mod+R", { when: () => false });
    const on = cmd("on", "global", "Mod+R");
    registerCommands([off, on]);
    expect(resolveKeydown(key("r", { meta: true }), ["global", "db"])?.id).toBe("on");
    expect(resolveKeydown(key("x", { meta: true }), ["global", "db"])).toBeNull();
  });

  it("fires only Mod combos (and Escape) while typing in an input", () => {
    registerCommands([cmd("plain", "global", "J"), cmd("mod", "global", "Mod+J"), cmd("esc", "global", "Escape")]);
    const input = document.createElement("input");
    expect(resolveKeydown(key("j", {}, input), ["global"])).toBeNull();
    expect(resolveKeydown(key("j", { meta: true }, input), ["global"])?.id).toBe("mod");
    expect(resolveKeydown(key("Escape", {}, input), ["global"])?.id).toBe("esc");
    expect(resolveKeydown(key("j", {}, document.createElement("div")), ["global"])?.id).toBe("plain");
  });

  it("honors user overrides, including unbinding", () => {
    registerCommands([cmd("a", "global", "Mod+K"), cmd("b", "global")]);
    setOverrides({ a: null, b: "Mod+Shift+X" });
    expect(bindingFor("a")).toBeNull();
    expect(bindingFor("b")).toBe("Mod+Shift+X");
    expect(resolveKeydown(key("k", { meta: true }), ["global"])).toBeNull();
    expect(resolveKeydown(key("x", { meta: true, shift: true }), ["global"])?.id).toBe("b");
  });
});

describe("conflicts", () => {
  afterEach(clearCommands);

  it("reports shared bindings only in overlapping scopes", () => {
    registerCommands([cmd("wp", "wp", "Mod+F"), cmd("table", "table", "Mod+F"), cmd("doc", "document", "Mod+E"), cmd("glob", "global", "Mod+E")]);
    const found = conflicts().map((c) => [c.a.id, c.b.id]);
    expect(found).toEqual([["doc", "glob"]]);
    expect(conflicts({ wp: "Mod+E" }).map((c) => [c.a.id, c.b.id])).toEqual([
      ["wp", "doc"],
      ["wp", "glob"],
      ["doc", "glob"],
    ]);
  });
});
