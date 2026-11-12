import { Blocks } from "lucide-react";
import { beforeEach, describe, expect, it } from "vitest";
import { installCommands } from "../commands";
import { findRow, pageRows, registerSettingsPage, resolveSettingsPage, searchSettings, settingsGroups } from "./settingsPages";

const hitIds = (query: string) => searchSettings(query).map(({ page, row }) => row?.id ?? page.id);

describe("resolveSettingsPage", () => {
  it.each([
    ["a page id", "status", "status"],
    ["a row id", "queries-row-limit", "queries"],
    ["a group id", "connections-addresses", "connections"],
    ["a row id on the Status page", "status-log-level", "status"],
  ])("resolves %s to its page", (_what, id, expected) => {
    expect(resolveSettingsPage(id)?.id).toBe(expected);
  });

  it("resolves a shortcut row id to the Shortcuts page", () => {
    installCommands();
    expect(resolveSettingsPage("shortcut:palette.open")?.id).toBe("shortcuts");
  });

  it.each([["an unknown id", "no-such-page"], ["no id", undefined]])("falls back to the first page for %s", (_what, id) => {
    expect(resolveSettingsPage(id)?.id).toBe("appearance");
  });
});

describe("pageRows and findRow", () => {
  it("reads rows given as a function when asked, so they follow other state", () => {
    let titles = ["First"];
    registerSettingsPage({ id: "test-dynamic", group: "Test Group", label: "Dynamic", icon: Blocks, rows: () => titles.map((title) => ({ id: `dyn-${title}`, title })), Body: () => null });
    const page = resolveSettingsPage("test-dynamic")!;
    expect(pageRows(page).map((r) => r.id)).toEqual(["dyn-First"]);
    titles = ["First", "Second"];
    expect(findRow(page, "dyn-Second")).toEqual({ id: "dyn-Second", title: "Second" });
  });

  it("finds no row on a page without rows", () => {
    registerSettingsPage({ id: "test-bare", group: "Test Group", label: "Bare", icon: Blocks, Body: () => null });
    const page = resolveSettingsPage("test-bare")!;
    expect(pageRows(page)).toEqual([]);
    expect(findRow(page, "anything")).toBeUndefined();
  });
});

describe("settingsGroups", () => {
  it("lists the known groups in nav order, each with its pages in order", () => {
    const groups = settingsGroups();
    expect(groups.slice(0, 3).map((g) => g.group)).toEqual(["Interface", "Database", "System"]);
    const labels = (name: string) => groups.find((g) => g.group === name)!.pages.map((p) => p.label);
    expect(labels("Interface")).toEqual(["Appearance", "Shortcuts"]);
    expect(labels("Database")).toEqual(["Queries", "Display", "Connections"]);
    expect(labels("System")).toEqual(["Status", "About"]);
  });
});

describe("searchSettings", () => {
  it("finds a row by its title, with the page it is on", () => {
    const [first] = searchSettings("dark");
    expect(first.page.id).toBe("appearance");
    expect(first.row).toMatchObject({ id: "appearance-dark-scheme", title: "Dark scheme" });
  });

  it("matches in any case and ignores surrounding spaces", () => {
    expect(hitIds("  DARK SCHEME ")[0]).toBe("appearance-dark-scheme");
  });

  it("finds a page by its label as a hit without a row", () => {
    const [first] = searchSettings("queries");
    expect(first.page.id).toBe("queries");
    expect(first.row).toBeUndefined();
  });

  it("finds a page by its group name", () => {
    expect(hitIds("database")).toEqual(expect.arrayContaining(["queries", "display", "connections"]));
  });

  it("finds a row by a keyword it does not show", () => {
    expect(hitIds("typeface")).toEqual(expect.arrayContaining(["appearance-font-ui", "appearance-font-mono"]));
  });

  it("finds the log level row on Status by 'logging'", () => {
    expect(hitIds("logging")).toContain("status-log-level");
  });

  it("finds a shortcut by its command id", () => {
    installCommands();
    expect(hitIds("view.sidebar")).toContain("shortcut:view.sidebar");
  });

  it("returns nothing when nothing matches", () => {
    expect(searchSettings("zzzz")).toEqual([]);
  });

  it("ranks a title starting with the query before a word start before letters in order", () => {
    registerSettingsPage({
      id: "rank-test",
      group: "Test Group",
      label: "Ranking",
      icon: Blocks,
      rows: [
        { id: "r-letters", title: "Stored ideas" },
        { id: "r-word", title: "Toggle sidebar" },
        { id: "r-start", title: "Sidebar width" },
      ],
      Body: () => null,
    });
    const onPage = searchSettings("side").filter(({ page }) => page.id === "rank-test");
    expect(onPage.map(({ row }) => row?.id)).toEqual(["r-start", "r-word", "r-letters"]);
  });
});

describe("pages registered later", () => {
  beforeEach(() => {
    registerSettingsPage({ id: "test-search", group: "Test Group", label: "Search target", icon: Blocks, rows: [{ id: "test-search-row", title: "Zebra stripes", under: "test-search-group" }], Body: () => null });
  });

  it("are found by their rows and resolved by row id", () => {
    expect(hitIds("zebra")).toContain("test-search-row");
    expect(resolveSettingsPage("test-search-row")?.id).toBe("test-search");
  });
});
