import { act } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { connectedStore, pendingChange } from "../test/fixtures";
import { registerSlice, resetStore, useStore, type Store } from "./store";

declare module "./store" {
  interface Store {
    probeNotes: Record<string, string>;
    probeLabel: string;
    setProbeLabel(label: string): void;
  }
}

registerSlice({
  create: (set) => ({
    probeNotes: {},
    probeLabel: "initial",
    setProbeLabel: (probeLabel: string) => set({ probeLabel }),
  }),
  sessionClosed: (sessionId, s) => {
    const { [sessionId]: _closed, ...probeNotes } = s.probeNotes;
    return { probeNotes };
  },
});

describe("slice registry", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
  });

  describe("registerSlice", () => {
    it("contributes the initial state and actions of a slice registered after the store was created", () => {
      const { probeLabel, setProbeLabel } = useStore.getState();

      expect(probeLabel).toBe("initial");
      setProbeLabel("changed");
      expect(useStore.getState().probeLabel).toBe("changed");
    });
  });

  describe("resetStore", () => {
    it("restores the initial state of a late slice and applies the patch on top", () => {
      useStore.getState().setProbeLabel("changed");

      resetStore({ activeSessionId: null });

      expect(useStore.getState().probeLabel).toBe("initial");
      expect(useStore.getState().activeSessionId).toBeNull();
    });
  });

  describe("disconnect", () => {
    it("applies every slice's sessionClosed result in the same update as the session removal", async () => {
      useStore.setState({ pending: [pendingChange()], probeNotes: { s1: "note" } });
      const notified: Store[] = [];
      const unsubscribe = useStore.subscribe((state) => notified.push(state));

      await act(() => useStore.getState().disconnect("s1"));
      unsubscribe();

      const sessionGone = notified.filter((state) => !state.sessions.some((x) => x.id === "s1"));
      expect(sessionGone.length).toBeGreaterThan(0);
      for (const state of sessionGone) {
        expect(state.pending).toEqual([]);
        expect(state.probeNotes).toEqual({});
        expect(state.tabs.filter((t) => t.sessionId === "s1")).toEqual([]);
      }
    });

    it("purges the state a late slice keyed by the closed session", async () => {
      useStore.setState({ probeNotes: { s1: "note", s2: "other" } });

      await act(() => useStore.getState().disconnect("s1"));

      expect(useStore.getState().probeNotes).toEqual({ s2: "other" });
    });
  });
});
