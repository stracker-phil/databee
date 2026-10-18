import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api, type Coverage } from "../../api";
import { connectedStore, session } from "../../test/fixtures";
import { resetStore, selectEffectiveAccent, useStore } from "../store";

const mocked = vi.mocked(api, true);
const store = () => useStore.getState();
const done = (coverage: Partial<Coverage> = {}) => ({ status: "done" as const, coverage: { total: 4, resolved: 3, zeroOrNull: 0, sampleUnresolved: [], timedOut: false, ...coverage } });

describe("reference coverage cache", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [] });
  });

  it("stores a result under its database and reference", () => {
    store().setCoverage("s1/db", "ref-a", { status: "running" });
    store().setCoverage("s1/db", "ref-b", done());
    expect(store().coverage["s1/db"].results).toEqual({ "ref-a": { status: "running" }, "ref-b": done() });
  });

  it("drops a result when it is set to null", () => {
    store().setCoverage("s1/db", "ref-a", { status: "running" });
    store().setCoverage("s1/db", "ref-a", null);
    expect(store().coverage["s1/db"].results).toEqual({});
  });

  it("forgets every result of the database on refresh, counts the refresh and leaves other databases alone", () => {
    store().setCoverage("s1/db", "ref-a", done());
    store().setCoverage("s1/other", "ref-a", done());
    store().refreshCoverage("s1/db");
    store().refreshCoverage("s1/db");
    expect(store().coverage["s1/db"]).toEqual({ generation: 2, results: {} });
    expect(store().coverage["s1/other"].results).toEqual({ "ref-a": done() });
  });
});

describe("database accents", () => {
  const sibling = { ...session, id: "s2" };

  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [], settings: { accent: "orange" } });
  });

  describe("selectEffectiveAccent", () => {
    it("is the app's accent without a session", () => {
      resetStore({ settings: store().settings, accents: { "shop/db": "blue" } });
      expect(selectEffectiveAccent(store())).toBe("orange");
    });

    it("is the database's own accent when it has one", () => {
      useStore.setState({ accents: { "shop/db": "blue" } });
      expect(selectEffectiveAccent(store())).toBe("blue");
    });

    it("is the app's accent when only another database has its own", () => {
      useStore.setState({ accents: { "shop/other": "blue" } });
      expect(selectEffectiveAccent(store())).toBe("orange");
    });

    it("is the app's accent when the session has no database", () => {
      useStore.setState({ sessions: [{ ...session, database: null }], accents: { "shop/db": "blue" } });
      expect(selectEffectiveAccent(store())).toBe("orange");
    });

    it("is shared by two sessions of the same connection and database", () => {
      useStore.setState({ sessions: [session, sibling], activeSessionId: "s2", accents: { "shop/db": "blue" } });
      expect(selectEffectiveAccent(store())).toBe("blue");
    });

    it("follows the active session's database after switching", async () => {
      useStore.setState({ accents: { "shop/shop2": "purple" } });
      expect(selectEffectiveAccent(store())).toBe("orange");
      await store().switchDatabase("s1", "shop2");
      expect(selectEffectiveAccent(store())).toBe("purple");
    });
  });

  describe("loadAccents", () => {
    it("stores the accents Rust reads from the projects file", async () => {
      mocked.projectAccents.mockResolvedValue({ "shop/db": "gold" });
      await store().loadAccents();
      expect(store().accents).toEqual({ "shop/db": "gold" });
    });

    it("keeps the accents when loading fails", async () => {
      useStore.setState({ accents: { "shop/db": "gold" } });
      mocked.projectAccents.mockRejectedValue({ kind: "io", message: "unreadable", code: null });
      await store().loadAccents();
      expect(store().accents).toEqual({ "shop/db": "gold" });
    });
  });

  describe("setAccent", () => {
    it("sends the session, database and accent to Rust and stores what it returns", async () => {
      await store().setAccent("s1", "db", "blue");
      expect(mocked.projectSetAccent).toHaveBeenCalledWith("s1", "db", "blue");
      expect(store().accents).toEqual({ "shop/db": "blue" });
    });

    it("applies at once to the effective accent", async () => {
      await store().setAccent("s1", "db", "blue");
      expect(selectEffectiveAccent(store())).toBe("blue");
    });

    it("removes the entry when the accent is null", async () => {
      useStore.setState({ accents: { "shop/db": "blue", "shop/other": "gold" } });
      await store().setAccent("s1", "db", null);
      expect(mocked.projectSetAccent).toHaveBeenCalledWith("s1", "db", null);
      expect(store().accents).toEqual({ "shop/other": "gold" });
    });

    it("does not reload any view", async () => {
      const before = store().dataVersion;
      await store().setAccent("s1", "db", "blue");
      expect(store().dataVersion).toBe(before);
    });

    it("keeps the accents when Rust rejects the change", async () => {
      mocked.projectSetAccent.mockRejectedValue({ kind: "io", message: "read-only", code: null });
      await store().setAccent("s1", "db", "blue");
      expect(store().accents).toEqual({});
    });

    it("does nothing for an unknown session", async () => {
      await store().setAccent("gone", "db", "blue");
      expect(mocked.projectSetAccent).not.toHaveBeenCalled();
    });
  });
});
