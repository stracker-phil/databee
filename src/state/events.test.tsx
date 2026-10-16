import { act } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { connectedStore } from "../test/fixtures";
import { emitSessionEvent, onSessionEvent, type SessionEvent } from "./events";
import { useStore } from "./store";

describe("session events", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
  });

  it("delivers databaseChanged to a subscriber when the database is switched and stops after unsubscribe", async () => {
    const received: SessionEvent[] = [];
    const unsubscribe = onSessionEvent((event) => received.push(event));

    await act(() => useStore.getState().switchDatabase("s1", "other"));
    expect(received).toEqual([{ type: "databaseChanged", sessionId: "s1", database: "other" }]);

    unsubscribe();
    await act(() => useStore.getState().switchDatabase("s1", "third"));
    expect(received).toHaveLength(1);
  });

  it("emits no databaseChanged when the server rejects the switch", async () => {
    const { api } = await import("../api");
    vi.mocked(api, true).useDatabase.mockRejectedValueOnce({ kind: "io", message: "unknown database", code: null });
    const received: SessionEvent[] = [];
    const unsubscribe = onSessionEvent((event) => received.push(event));

    await act(() => useStore.getState().switchDatabase("s1", "missing"));
    unsubscribe();

    expect(received).toEqual([]);
  });

  it("delivers sessionClosed after the session is removed on disconnect", async () => {
    const sessionsAtEmit: string[][] = [];
    const unsubscribe = onSessionEvent((event) => {
      if (event.type === "sessionClosed") sessionsAtEmit.push(useStore.getState().sessions.map((s) => s.id));
    });

    await act(() => useStore.getState().disconnect("s1"));
    unsubscribe();

    expect(sessionsAtEmit).toEqual([[]]);
  });

  it("calls every subscriber for an emitted event", () => {
    const first = vi.fn();
    const second = vi.fn();
    const offFirst = onSessionEvent(first);
    const offSecond = onSessionEvent(second);

    emitSessionEvent({ type: "sessionClosed", sessionId: "s9" });
    offFirst();
    offSecond();

    expect(first).toHaveBeenCalledWith({ type: "sessionClosed", sessionId: "s9" });
    expect(second).toHaveBeenCalledWith({ type: "sessionClosed", sessionId: "s9" });
  });
});
