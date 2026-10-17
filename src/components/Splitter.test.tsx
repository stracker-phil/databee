import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import App from "../App";
import { api } from "../api";
import { resetStore, useStore } from "../state/store";
import { columnMeta, connectedStore, session, text } from "../test/fixtures";

const mocked = vi.mocked(api, true);

const inspect = () => useStore.setState({ selectedCell: { tabId: "q1", row: 0, col: 0, column: columnMeta("option_value", "LONGTEXT"), cell: text('a:1:{s:1:"k";s:1:"v";}'), source: { sessionId: "s1", resultId: 1 } } });

function drag(handle: HTMLElement, from: { x?: number; y?: number }, to: { x?: number; y?: number }) {
  fireEvent.mouseDown(handle, { button: 0, clientX: from.x ?? 0, clientY: from.y ?? 0 });
  fireEvent.mouseMove(window, { clientX: to.x ?? 0, clientY: to.y ?? 0 });
  fireEvent.mouseUp(window, { clientX: to.x ?? 0, clientY: to.y ?? 0 });
}

describe("resizable panes", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetStore();
    window.innerWidth = 1600;
    window.innerHeight = 1000;
  });

  it("widens the inspector by dragging its edge and keeps the width per connection", async () => {
    connectedStore();
    inspect();
    render(<App />);
    const handle = screen.getByRole("separator", { name: "Resize inspector" });
    expect(handle).toHaveAttribute("aria-orientation", "vertical");
    expect(handle).toHaveAttribute("aria-valuenow", "360");
    drag(handle, { x: 1200 }, { x: 1000 });
    expect(handle).toHaveAttribute("aria-valuenow", "560");
    await vi.waitFor(() => expect(mocked.setUiState).toHaveBeenLastCalledWith(session.identity, expect.objectContaining({ inspectorWidth: 560 })), { timeout: 1500 });
  });

  it("docks the inspector below the tabs and resizes its height", async () => {
    connectedStore({ settings: { inspectorDock: "bottom" } });
    inspect();
    render(<App />);
    expect(screen.getByRole("complementary", { name: "Value inspector" })).toBeInTheDocument();
    const handle = screen.getByRole("separator", { name: "Resize inspector" });
    expect(handle).toHaveAttribute("aria-orientation", "horizontal");
    drag(handle, { y: 600 }, { y: 400 });
    await vi.waitFor(() => expect(mocked.setUiState).toHaveBeenLastCalledWith(session.identity, expect.objectContaining({ bottomPaneHeight: 520 })), { timeout: 1500 });
  });

  it("resizes the SQL editor against the results, also with the keyboard", async () => {
    connectedStore();
    render(<App />);
    const handle = screen.getByRole("separator", { name: "Resize editor" });
    expect(handle).toHaveAttribute("aria-valuenow", "140");
    drag(handle, { y: 300 }, { y: 220 });
    expect(handle).toHaveAttribute("aria-valuenow", "60");
    fireEvent.keyDown(handle, { key: "ArrowDown" });
    await vi.waitFor(() => expect(mocked.setUiState).toHaveBeenLastCalledWith(session.identity, expect.objectContaining({ sqlEditorHeight: 76 })), { timeout: 1500 });
  });

  it("toggles the dock from the View menu command", async () => {
    connectedStore();
    const { installCommands, runCommand } = await import("../commands");
    installCommands();
    runCommand("view.inspectorDock");
    await vi.waitFor(() => expect(useStore.getState().settings?.inspectorDock).toBe("bottom"));
  });
});
