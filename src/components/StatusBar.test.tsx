import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { installCommands } from "../commands";
import { selectSidebarHidden, useStore, type TableTab } from "../state/store";
import { connectedStore, pendingChange } from "../test/fixtures";
import { StatusBar } from "./StatusBar";

describe("StatusBar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
    installCommands();
  });

  it("has no theme or inspector chips", () => {
    render(<StatusBar />);
    expect(screen.queryByRole("button", { name: /Theme/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Inspector/ })).not.toBeInTheDocument();
  });

  it("hides the sidebar from its toggle and reports the button as not pressed", async () => {
    render(<StatusBar />);
    const toggle = screen.getByRole("button", { name: "Toggle sidebar" });
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-pressed", "false");
    expect(selectSidebarHidden(useStore.getState())).toBe(true);
  });

  it("shows a table tab's address as database.table", () => {
    const tab: TableTab = { id: "t1", kind: "table", sessionId: "s1", title: "wp_posts", database: "db", table: "wp_posts", view: "data", where: [] };
    useStore.setState({ tabs: [tab], activeTabId: "t1" });
    render(<StatusBar />);
    expect(screen.getByText("db.wp_posts")).toBeInTheDocument();
  });

  it("shows plain text and no button when nothing is pending", () => {
    render(<StatusBar />);
    expect(screen.getByText("pending 0")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /pending/ })).not.toBeInTheDocument();
  });

  it("flags pending changes", () => {
    useStore.setState({ pending: [pendingChange()] });
    render(<StatusBar />);
    expect(screen.getByRole("button", { name: "1 pending change" })).toBeInTheDocument();
  });

  it("counts only the active session's pending changes", () => {
    useStore.setState({ pending: [pendingChange({ id: 1 }), pendingChange({ id: 2, sessionId: "s2" }), pendingChange({ id: 3, sessionId: "s2" })] });
    render(<StatusBar />);
    expect(screen.getByRole("button", { name: "1 pending change" })).toBeInTheDocument();
  });

  it("shows no pending chip when only another session has changes", () => {
    useStore.setState({ pending: [pendingChange({ sessionId: "s2" })] });
    render(<StatusBar />);
    expect(screen.queryByRole("button", { name: /pending/ })).not.toBeInTheDocument();
  });
});
