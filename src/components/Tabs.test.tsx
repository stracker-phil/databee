import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { installCommands } from "../commands";
import { useStore, type TableTab } from "../state/store";
import { connectedStore, queryTab } from "../test/fixtures";
import { TabBar, TabPanels } from "./Tabs";

const tabNamed = (title: RegExp) => screen.getByRole("tab", { name: title });

describe("TabPanels empty state", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [] });
    installCommands();
  });

  it("hints at the default shortcut for a new query tab", () => {
    render(<TabPanels />);
    expect(screen.getByText(/press ⌘T for a new query/)).toBeInTheDocument();
  });

  it("follows a rebinding of tabs.new", async () => {
    render(<TabPanels />);
    await act(() => useStore.getState().saveKeybindings({ "tabs.new": "Mod+Shift+N" }));
    expect(screen.getByText(/press ⌘⇧N for a new query/)).toBeInTheDocument();
  });

  it("points at the plus button when tabs.new is unbound", async () => {
    render(<TabPanels />);
    await act(() => useStore.getState().saveKeybindings({ "tabs.new": null }));
    expect(screen.getByText(/click \+ for a new query/)).toBeInTheDocument();
  });
});

describe("TabBar", () => {
  const previewTab: TableTab = {
    id: "t1",
    kind: "table",
    sessionId: "s1",
    title: "wp_posts",
    database: "db",
    table: "wp_posts",
    view: "data",
    preview: true,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [queryTab({ id: "q1", title: "Query 1" }), queryTab({ id: "q2", title: "Query 2" })] });
    installCommands();
  });

  it("closes a tab on middle-click", () => {
    render(<TabBar />);
    fireEvent(tabNamed(/Query 2/), new MouseEvent("auxclick", { bubbles: true, button: 1 }));
    expect(screen.queryByRole("tab", { name: /Query 2/ })).not.toBeInTheDocument();
    expect(tabNamed(/Query 1/)).toBeInTheDocument();
  });

  it("keeps a tab open when another mouse button is released on it", () => {
    render(<TabBar />);
    fireEvent(tabNamed(/Query 2/), new MouseEvent("auxclick", { bubbles: true, button: 2 }));
    expect(tabNamed(/Query 2/)).toBeInTheDocument();
  });

  it("marks a preview tab and keeps it on double-click", async () => {
    useStore.setState({ tabs: [previewTab], activeTabId: previewTab.id });
    render(<TabBar />);
    expect(tabNamed(/wp_posts/)).toHaveAttribute("title", expect.stringContaining("preview (double-click to keep)"));
    await userEvent.dblClick(tabNamed(/wp_posts/));
    expect(tabNamed(/wp_posts/)).toHaveAttribute("title", "db.wp_posts");
  });

  it("makes only the active tab reachable with Tab", () => {
    render(<TabBar />);
    expect(tabNamed(/Query 1/)).toHaveAttribute("tabindex", "0");
    expect(tabNamed(/Query 2/)).toHaveAttribute("tabindex", "-1");
  });

  it("activates and focuses the next tab on ArrowRight", async () => {
    render(<TabBar />);
    await userEvent.tab();
    expect(tabNamed(/Query 1/)).toHaveFocus();
    await userEvent.keyboard("{ArrowRight}");
    expect(tabNamed(/Query 2/)).toHaveAttribute("aria-selected", "true");
    expect(tabNamed(/Query 2/)).toHaveFocus();
    expect(tabNamed(/Query 2/)).toHaveAttribute("tabindex", "0");
  });

  describe("with tabs of two sessions", () => {
    beforeEach(() => {
      connectedStore({
        tabs: [queryTab({ id: "q1", title: "Query 1" }), queryTab({ id: "q2", title: "Blog query", sessionId: "s2" }), queryTab({ id: "q3", title: "Query 3" })],
      });
    });

    it("activates a tab of the same session after closing the active one", async () => {
      render(<TabBar />);
      await userEvent.click(screen.getByRole("button", { name: "Close Query 1" }));
      expect(tabNamed(/Query 3/)).toHaveAttribute("aria-selected", "true");
    });

    it("activates no tab when the closed one was the last of its session", async () => {
      connectedStore({ tabs: [queryTab({ id: "q1", title: "Query 1" }), queryTab({ id: "q2", title: "Blog query", sessionId: "s2" })] });
      render(<TabBar />);
      await userEvent.click(screen.getByRole("button", { name: "Close Query 1" }));
      expect(useStore.getState().activeTabId).toBeNull();
    });
  });
});
