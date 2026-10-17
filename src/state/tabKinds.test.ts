import { beforeEach, describe, expect, it } from "vitest";
import type { Address } from "../api";
import { objectColumn, type StackColumn } from "../lib/columns";
import { connectedStore, queryTab, readoutTab, session, tableTab, value } from "../test/fixtures";
import { resetStore, useStore, type ObjectTab, type ProjectTab, type Tab } from "./store";
import { tabDatabase, tabKind } from "./tabKinds";

const rowAddress = (database: string, key: string): Address => ({
  connection: session.slug,
  database,
  target: { kind: "row", table: "wp_posts", key },
  view: null,
  compare: null,
});

const valueColumn = (): StackColumn => ({ kind: "value", value: value(), text: "x", database: null });

function objectTab(columns: StackColumn[], active: number): ObjectTab {
  return {
    id: "o1",
    kind: "object",
    sessionId: session.id,
    title: "wp_posts #1",
    columns,
    active,
    address: rowAddress("db", "1"),
    stacks: {},
    nav: { back: [], forward: [] },
  };
}

function projectTab(database: string): ProjectTab {
  return {
    id: "p1",
    kind: "project",
    sessionId: session.id,
    title: "project",
    database,
    address: { connection: session.slug, database, target: { kind: "project" }, view: null, compare: null },
    form: null,
    nav: { back: [], forward: [] },
  };
}

const databaseOf = (tab: Tab, column?: number) => tabDatabase(useStore.getState(), tab, column);

describe("tabDatabase", () => {
  beforeEach(() => connectedStore());

  it("is a table tab's database", () => {
    expect(databaseOf(tableTab({ database: "shop" }))).toBe("shop");
  });

  describe("object tab", () => {
    const stack = [objectColumn(rowAddress("one", "1")), objectColumn(rowAddress("two", "2"))];

    it("is the database of the active column's row", () => {
      expect(databaseOf(objectTab(stack, 1))).toBe("two");
      expect(databaseOf(objectTab(stack, 0))).toBe("one");
    });

    it("is the database of the given column's row", () => {
      expect(databaseOf(objectTab(stack, 0), 1)).toBe("two");
      expect(databaseOf(objectTab(stack, 1), 0)).toBe("one");
    });

    it("takes a value column's database from the nearest object column before it", () => {
      const tab = objectTab([stack[0], stack[1], valueColumn()], 2);
      expect(databaseOf(tab)).toBe("two");
      expect(databaseOf(tab, 2)).toBe("two");
    });

    it("falls back to the tab's address when no object column precedes the index", () => {
      expect(databaseOf(objectTab([valueColumn()], 0))).toBe("db");
    });
  });

  it.each([
    ["project", projectTab("shop")],
    ["readout", readoutTab({ database: "shop" })],
  ])("is a %s tab's own database, not the session's", (_, tab) => {
    resetStore({ sessions: [{ ...session, database: "other" }] });
    expect(databaseOf(tab)).toBe("shop");
  });

  describe("query tab", () => {
    it("is the database of the tab's session", () => {
      resetStore({ sessions: [{ ...session, database: "current" }] });
      expect(databaseOf(queryTab())).toBe("current");
    });

    it("is null when the session has no database", () => {
      resetStore({ sessions: [{ ...session, database: null }] });
      expect(databaseOf(queryTab())).toBeNull();
    });

    it("is null when the session is gone", () => {
      resetStore({ sessions: [] });
      expect(databaseOf(queryTab())).toBeNull();
    });
  });
});

describe("tabKind", () => {
  it.each([
    ["table", tableTab(), true],
    ["object", objectTab([], 0), true],
    ["query", queryTab(), false],
    ["project", projectTab("shop"), false],
    ["readout", readoutTab(), false],
  ])("marks a %s tab previewable: %s", (_kind, tab, preview) => {
    expect(tabKind(tab).preview).toBe(preview);
  });
});
