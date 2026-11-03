import { describe, expect, it } from "vitest";
import { resolveSettingsPage, settingsGroups } from "./settingsPages";

const labels = (query: string) => settingsGroups(query).flatMap((g) => g.pages.map((p) => p.page.label));

describe("resolveSettingsPage", () => {
  it.each([
    ["a page id", "logging", "logging"],
    ["a section id", "queries-limits", "queries"],
    ["a section id of the second section", "connections-addresses", "connections"],
  ])("resolves %s to its page", (_what, id, expected) => {
    expect(resolveSettingsPage(id)?.id).toBe(expected);
  });

  it.each([["an unknown id", "no-such-page"], ["no id", undefined]])("falls back to the first page for %s", (_what, id) => {
    expect(resolveSettingsPage(id)?.id).toBe("appearance");
  });
});

describe("settingsGroups", () => {
  it("lists every page under its group in nav order without a query", () => {
    const names = settingsGroups().map((g) => g.group);
    expect(names.slice(0, 3)).toEqual(["Interface", "Database", "System"]);
    expect(settingsGroups()[0].pages.map((p) => p.page.label)).toEqual(["Appearance", "Shortcuts"]);
    expect(settingsGroups().flatMap((g) => g.pages).every((p) => p.section === undefined)).toBe(true);
  });

  it("matches a page label in any case and ignores surrounding spaces", () => {
    expect(labels("  LOGGING ")).toEqual(["Logging"]);
    expect(settingsGroups("LOGGING")[0].pages[0].section).toBeUndefined();
  });

  it("matches every page of a group by the group name", () => {
    expect(labels("database")).toEqual(["Queries", "Display", "Connections"]);
  });

  it("finds a page by a section title and names the section", () => {
    const [group] = settingsGroups("look");
    expect(group.group).toBe("Interface");
    expect(group.pages).toEqual([expect.objectContaining({ page: expect.objectContaining({ id: "appearance" }), section: { id: "appearance-look", title: "Look" } })]);
  });

  it("prefers the page label over a section when both match", () => {
    const [group] = settingsGroups("queries");
    expect(group.pages[0].page.id).toBe("queries");
    expect(group.pages[0].section).toBeUndefined();
  });

  it("returns no groups when nothing matches", () => {
    expect(settingsGroups("zzzz")).toEqual([]);
  });
});
