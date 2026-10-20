import { act } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api } from "../api";
import { useStore } from "../state/store";
import { connectedStore, queryTab, tableTab } from "../test/fixtures";
import { objectDocument } from "../test/objectFixtures";
import { clearCommands, installCommands, runMenuCommand } from "./index";

const mocked = vi.mocked(api, true);

const dialogTitle = () => {
  const dialog = useStore.getState().dialog;
  return dialog?.type === "prompt" ? dialog.title : null;
};

describe("runMenuCommand", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocked.document.mockResolvedValue(objectDocument());
    connectedStore({ tabs: [queryTab({ saved: { id: "sq1", origin: { kind: "project" }, database: "db", pin: null } })] });
    installCommands();
  });

  afterEach(() => clearCommands());

  it("runs the query variant of a shared item while a saved query tab is active", () => {
    expect(runMenuCommand("object.snapshot")).toBe(true);
    expect(dialogTitle()).toBe("Snapshot of Query 1");
  });

  it("runs the object variant of a shared item while an object tab is active", async () => {
    await act(() => useStore.getState().handleLaunchEvents([{ type: "open", address: "databee://shop/db/wp_wc_orders/4711", connection: null }]));
    expect(runMenuCommand("object.snapshot")).toBe(true);
    expect(dialogTitle()).toBe("Snapshot of wp_wc_orders #4711");
  });

  it("runs a command by its own id", () => {
    expect(runMenuCommand("tabs.new")).toBe(true);
    expect(useStore.getState().tabs).toHaveLength(2);
  });

  it("does not run an item whose scope is not active", () => {
    useStore.setState({ tabs: [tableTab()], activeTabId: "t1" });
    expect(runMenuCommand("object.snapshot")).toBe(false);
    expect(useStore.getState().dialog).toBeNull();
  });

  it("does not run a command whose condition fails", () => {
    useStore.setState({ tabs: [queryTab()], activeTabId: "q1" });
    expect(runMenuCommand("object.snapshot")).toBe(false);
    expect(useStore.getState().dialog).toBeNull();
  });

  it("returns false for an unknown id", () => {
    expect(runMenuCommand("no.such.command")).toBe(false);
  });

  it("yields to an open dialog except for the commands that stay allowed", () => {
    useStore.getState().openDialog({ type: "prompt", title: "Open", value: "", onSubmit: () => {} });
    expect(runMenuCommand("tabs.new")).toBe(false);
    expect(useStore.getState().tabs).toHaveLength(1);
    expect(runMenuCommand("view.zoomReset")).toBe(true);
  });
});
