import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import type { Coverage } from "../../api";
import { connectedStore } from "../../test/fixtures";
import { useStore } from "../store";

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
