import { act } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api } from "../../api";
import { installCommands, runCommand } from "../../commands";
import { formatAddress } from "../../lib/address";
import { connectedStore, queryTab, readoutOf, readoutTab, session } from "../../test/fixtures";
import { resetStore, selectActiveTab, useStore, type ReadoutTab } from "../store";

const mocked = vi.mocked(api, true);
const store = () => useStore.getState();
const readoutTabs = () => store().tabs.filter((t): t is ReadoutTab => t.kind === "readout");

describe("readout tab", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    installCommands();
  });

  describe("connect", () => {
    beforeEach(() => {
      resetStore();
      mocked.connect.mockResolvedValue(session);
    });

    it("opens the readout of the current database as the active tab next to a query tab", async () => {
      await act(() => store().connect({ spec: session.spec }));
      expect(store().tabs.map((t) => t.kind).sort()).toEqual(["query", "readout"]);
      expect(selectActiveTab(store())).toBe(readoutTabs()[0]);
      expect(readoutTabs()[0]).toMatchObject({ database: "db", title: "readout" });
      expect(formatAddress(readoutTabs()[0].address)).toBe("databee://shop/db/");
    });

    it("shows the one readout of a session again when it connects a second time", async () => {
      await act(() => store().connect({ spec: session.spec }));
      store().activateTab(store().tabs.find((t) => t.kind === "query")!.id);
      await act(() => store().connect({ spec: session.spec }));
      expect(readoutTabs()).toHaveLength(1);
      expect(store().tabs.filter((t) => t.kind === "query")).toHaveLength(1);
      expect(selectActiveTab(store())?.kind).toBe("readout");
    });
  });

  describe("home address", () => {
    beforeEach(() => connectedStore({ tabs: [queryTab()] }));

    it("opens the readout of the address", async () => {
      await act(() => store().openAddress("databee://shop/db/"));
      expect(selectActiveTab(store())).toMatchObject({ kind: "readout", database: "db" });
    });

    it("moves the existing readout to the database the address names", async () => {
      await act(() => store().openAddress("databee://shop/db/"));
      await act(() => store().openAddress("databee://shop/other/"));
      expect(readoutTabs()).toHaveLength(1);
      expect(readoutTabs()[0]).toMatchObject({ database: "other" });
      expect(formatAddress(readoutTabs()[0].address)).toBe("databee://shop/other/");
    });

    it("toasts when no open connection has the slug", async () => {
      await act(() => store().openAddress("databee://ghost/db/"));
      expect(readoutTabs()).toEqual([]);
      expect(store().toasts.at(-1)?.message).toBe("No open connection is named ghost");
    });
  });

  describe("switchDatabase", () => {
    it("moves an existing readout to the new database without activating it", async () => {
      connectedStore({ tabs: [readoutTab(), queryTab()] });
      store().activateTab("q1");
      await act(() => store().switchDatabase("s1", "other"));
      expect(readoutTabs()[0]).toMatchObject({ database: "other" });
      expect(formatAddress(readoutTabs()[0].address)).toBe("databee://shop/other/");
      expect(store().activeTabId).toBe("q1");
    });

    it("opens no readout when the session has none", async () => {
      connectedStore({ tabs: [queryTab()] });
      await act(() => store().switchDatabase("s1", "other"));
      expect(readoutTabs()).toEqual([]);
    });
  });

  describe("readout.open", () => {
    it("opens and activates the readout of the current database", () => {
      connectedStore({ tabs: [queryTab()] });
      expect(runCommand("readout.open")).toBe(true);
      expect(selectActiveTab(store())).toMatchObject({ kind: "readout", database: "db" });
    });

    it("returns to the existing readout instead of opening a second one", () => {
      connectedStore({ tabs: [readoutTab(), queryTab()] });
      store().activateTab("q1");
      runCommand("readout.open");
      expect(store().activeTabId).toBe("r1");
      expect(readoutTabs()).toHaveLength(1);
    });

    it("cannot run without a connection", () => {
      resetStore();
      expect(runCommand("readout.open")).toBe(false);
    });
  });

  describe("watched values", () => {
    const watched = { table: "wp_options", column: "option_value", row: [{ column: "option_name", value: "cfg" }] };
    const rejection = { kind: "io", message: "gone", code: null };

    beforeEach(() => connectedStore({ tabs: [readoutTab()] }));

    it("watches the node of a value, toasts, and reloads a loaded readout", async () => {
      await store().loadReadout("s1", "db");
      mocked.readout.mockClear();
      await store().watchValue("s1", "db", watched, ["mode"]);
      expect(mocked.readoutWatch).toHaveBeenCalledWith("s1", "db", watched, ["mode"]);
      expect(store().toasts.at(-1)?.message).toBe("Watching wp_options.option_value on the readout");
      expect(mocked.readout).toHaveBeenCalledWith("s1", "db");
    });

    it("does not load a readout that was never loaded", async () => {
      await store().watchValue("s1", "db", watched, []);
      expect(mocked.readoutWatch).toHaveBeenCalled();
      expect(mocked.readout).not.toHaveBeenCalled();
    });

    it("toasts the failure of watching and shows no success", async () => {
      mocked.readoutWatch.mockRejectedValueOnce(rejection);
      await store().watchValue("s1", "db", watched, []);
      expect(store().toasts.map((t) => t.message)).toEqual(["Could not watch the value: gone"]);
    });

    it("removes a watched value and reloads a loaded readout", async () => {
      await store().loadReadout("s1", "db");
      mocked.readout.mockClear();
      await store().unwatchValue("s1", "db", "w1");
      expect(mocked.readoutUnwatch).toHaveBeenCalledWith("s1", "db", "w1");
      expect(mocked.readout).toHaveBeenCalledWith("s1", "db");
    });

    it("toasts the failure of removing", async () => {
      mocked.readoutUnwatch.mockRejectedValueOnce(rejection);
      await store().unwatchValue("s1", "db", "w1");
      expect(store().toasts.at(-1)?.message).toBe("Could not remove the watched value: gone");
    });

    it("renames a watched value, with null for the default label, and reloads a loaded readout", async () => {
      await store().loadReadout("s1", "db");
      mocked.readout.mockClear();
      await store().labelWatched("s1", "db", "w1", null);
      expect(mocked.readoutWatchLabel).toHaveBeenCalledWith("s1", "db", "w1", null);
      expect(mocked.readout).toHaveBeenCalledWith("s1", "db");
    });

    it("toasts the failure of renaming", async () => {
      mocked.readoutWatchLabel.mockRejectedValueOnce(rejection);
      await store().labelWatched("s1", "db", "w1", "Mode");
      expect(store().toasts.at(-1)?.message).toBe("Could not rename the watched value: gone");
    });
  });

  describe("loadReadout", () => {
    beforeEach(() => connectedStore({ tabs: [readoutTab()] }));

    it("asks Rust once per data version unless forced", async () => {
      await store().loadReadout("s1", "db");
      await store().loadReadout("s1", "db");
      expect(mocked.readout).toHaveBeenCalledTimes(1);
      await store().loadReadout("s1", "db", true);
      expect(mocked.readout).toHaveBeenCalledTimes(2);
      store().bumpData();
      await store().loadReadout("s1", "db");
      expect(mocked.readout).toHaveBeenCalledTimes(3);
    });

    it("stores the error when Rust rejects the request", async () => {
      mocked.readout.mockRejectedValueOnce({ kind: "io", message: "gone", code: null });
      await store().loadReadout("s1", "db");
      expect(store().readouts["s1/db"]).toMatchObject({ readout: null, error: { message: "gone" } });
    });

    it("drops the readouts of a disconnected session", async () => {
      mocked.readout.mockResolvedValue(readoutOf());
      await store().loadReadout("s1", "db");
      await act(() => store().disconnect("s1"));
      expect(store().readouts).toEqual({});
    });
  });
});
