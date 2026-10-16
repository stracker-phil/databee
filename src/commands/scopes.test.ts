import { afterEach, describe, expect, it, vi } from "vitest";
import { clearCommands, conflicts, registerCommands, resolveKeydown, type Command } from "./registry";
import { activeScopes, isAncestor, registerScope, scopeLabel, specificity } from "./scopes";

const cmd = (id: string, scope: string, defaultBinding?: string): Command => ({ id, title: id, scope, defaultBinding, run: vi.fn() });
const cmdK = { key: "k", metaKey: true, ctrlKey: false, altKey: false, shiftKey: false };

describe("scope registry", () => {
  it("measures specificity as the depth in the parent chain", () => {
    expect(specificity("global")).toBe(0);
    expect(specificity("db")).toBe(1);
    expect(specificity("table")).toBe(2);
    expect(specificity("document")).toBe(2);
  });

  it("knows ancestors along the whole chain but not siblings or itself", () => {
    expect(isAncestor("global", "table")).toBe(true);
    expect(isAncestor("db", "table")).toBe(true);
    expect(isAncestor("table", "db")).toBe(false);
    expect(isAncestor("table", "editor")).toBe(false);
    expect(isAncestor("table", "table")).toBe(false);
  });

  it("throws when a scope names a parent that is not registered", () => {
    expect(() => registerScope({ id: "orphan", parent: "no-such-scope", label: "Orphan", isActive: () => true })).toThrow(/unknown parent "no-such-scope"/);
    expect(scopeLabel("orphan")).toBe("orphan");
  });

  it("labels a registered scope and falls back to the id for an unknown one", () => {
    registerScope({ id: "labelled", parent: "global", label: "Labelled", isActive: () => false });
    expect(scopeLabel("labelled")).toBe("Labelled");
    expect(scopeLabel("never-registered")).toBe("never-registered");
  });

  it("lists only the scopes whose check passes", () => {
    let on = false;
    registerScope({ id: "toggled", parent: "global", label: "Toggled", isActive: () => on });
    expect(activeScopes()).toContain("global");
    expect(activeScopes()).not.toContain("toggled");
    on = true;
    expect(activeScopes()).toContain("toggled");
  });
});

describe("commands and scopes", () => {
  afterEach(clearCommands);

  it("throws when registering a command with an unregistered scope", () => {
    expect(() => registerCommands([cmd("ok", "global"), cmd("typo", "tabel")])).toThrow(/"typo".*unknown scope "tabel"/);
  });

  it("resolves a table command over a global one bound to the same key", () => {
    registerCommands([cmd("everywhere", "global", "Mod+K"), cmd("in-table", "table", "Mod+K")]);
    expect(resolveKeydown(cmdK, ["global", "db", "table"])?.id).toBe("in-table");
  });

  it("does not resolve a command whose scope is inactive", () => {
    let on = false;
    registerScope({ id: "toggled-cmd", parent: "global", label: "Toggled", isActive: () => on });
    registerCommands([cmd("scoped", "toggled-cmd", "Mod+K")]);
    expect(resolveKeydown(cmdK, activeScopes())).toBeNull();
    on = true;
    expect(resolveKeydown(cmdK, activeScopes())?.id).toBe("scoped");
  });

  it("reports a shared binding between parent and child scopes but not between siblings", () => {
    registerCommands([cmd("table", "table", "Mod+K"), cmd("editor", "editor", "Mod+K"), cmd("db", "db", "Mod+K")]);
    expect(conflicts().map((c) => [c.a.id, c.b.id])).toEqual([
      ["table", "db"],
      ["editor", "db"],
    ]);
  });
});
