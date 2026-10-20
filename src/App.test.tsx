import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", async (orig) => ({ ...(await orig<object>()), api: (await import("./test/apiMock")).createApiMock() }));

import App from "./App";
import { api, EMPTY_UI_STATE, type SessionInfo } from "./api";
import { runCommand } from "./commands";
import { resetStore, selectSidebarHidden, useStore } from "./state/store";
import { connectedStore, queryTab, resultSet, session } from "./test/fixtures";

const mocked = vi.mocked(api, true);

describe("App", () => {
  beforeEach(() => resetStore());

  it("shows the connect screen when nothing is connected", async () => {
    render(<App />);
    expect(await screen.findByRole("heading", { name: "New connection" })).toBeInTheDocument();
  });

  it("renders the workspace for a connected session", async () => {
    connectedStore();
    render(<App />);
    expect(screen.getByText("Shop")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /Query 1/ })).toBeInTheDocument();
    expect(await screen.findByText("wp_options")).toBeInTheDocument();
    expect(screen.getByTitle(/root@127\.0\.0\.1:52958\/db · .*10\.3\.39-MariaDB/)).toHaveTextContent("Shop");
  });

  it("runs menu commands by id", async () => {
    connectedStore();
    render(<App />);
    await screen.findByText("wp_options");
    await act(() => runCommand("tabs.new"));
    expect(screen.getAllByRole("tab")).toHaveLength(2);
    await act(() => runCommand("tabs.close"));
    expect(screen.getAllByRole("tab")).toHaveLength(1);
    await act(() => runCommand("settings.open"));
    expect(await screen.findByRole("heading", { name: "Appearance" })).toBeInTheDocument();
  });

  it("resolves shortcuts through the registry in both views", async () => {
    connectedStore();
    render(<App />);
    await userEvent.keyboard("{Meta>}k{/Meta}");
    expect(await screen.findByLabelText("Search everything")).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    await userEvent.keyboard("{Meta>}t{/Meta}");
    expect(screen.getAllByRole("tab").map((t) => t.getAttribute("aria-selected"))).toEqual(["false", "true"]);
    await userEvent.keyboard("{Control>}{Tab}{/Control}");
    expect(screen.getAllByRole("tab").map((t) => t.getAttribute("aria-selected"))).toEqual(["true", "false"]);
    await userEvent.keyboard("{Meta>}={/Meta}");
    expect(useStore.getState().settings?.uiFontSize).toBe(14);
    expect(useStore.getState().settings?.dataFontSize).toBe(13);
  });

  it("applies rebound shortcuts without a restart", async () => {
    connectedStore();
    render(<App />);
    await screen.findByText("wp_options");
    await useStore.getState().saveKeybindings({ "palette.open": "Mod+J" });
    await userEvent.keyboard("{Meta>}k{/Meta}");
    expect(screen.queryByLabelText("Search everything")).not.toBeInTheDocument();
    await userEvent.keyboard("{Meta>}j{/Meta}");
    expect(await screen.findByLabelText("Search everything")).toBeInTheDocument();
  });

  it("toasts the message of a failed launch request instead of an object", async () => {
    vi.clearAllMocks();
    mocked.takeLaunchEvents.mockRejectedValueOnce({ kind: "io", message: "socket closed", code: null });
    render(<App />);
    expect(await screen.findByText("Launch request failed: socket closed")).toBeInTheDocument();
    expect(screen.queryByText(/\[object Object\]/)).not.toBeInTheDocument();
  });

  describe("connection health", () => {
    const down = { state: "down", since: 1_700_000_000_000, error: "Connection refused", retryAt: 1_700_000_030_000 } as const;

    beforeEach(() => {
      vi.clearAllMocks();
      connectedStore();
    });

    it("shows the warning when Rust reports the connection lost and removes it when it is back", async () => {
      render(<App />);
      await screen.findByText("wp_options");
      const report = mocked.onSessionHealth.mock.calls.at(-1)![0];

      act(() => report({ sessionId: "s1", health: down }));
      expect(await screen.findByRole("button", { name: "Retry" })).toBeInTheDocument();
      expect(await screen.findByText("Lost the connection to Shop")).toBeInTheDocument();

      await act(async () => report({ sessionId: "s1", health: { state: "ok" } }));
      expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
      expect(await screen.findByText("Reconnected to Shop")).toBeInTheDocument();
    });

    it("retries a lost session when the window regains focus", async () => {
      useStore.setState({ health: { s1: down } });
      render(<App />);
      await screen.findByText("wp_options");

      act(() => void fireEvent.focus(window));

      expect(mocked.reconnectSession).toHaveBeenCalledWith("s1");
    });

    it("does not reconnect anything on focus while every connection is fine", async () => {
      render(<App />);
      await screen.findByText("wp_options");

      act(() => void fireEvent.focus(window));

      expect(mocked.reconnectSession).not.toHaveBeenCalled();
    });
  });

  describe("sidebar toggle", () => {
    const tree = () => screen.queryByRole("tree", { name: "Databases" });

    beforeEach(() => {
      vi.clearAllMocks();
      connectedStore();
    });

    it("hides the sidebar with view.sidebar and persists the choice for the connection", async () => {
      render(<App />);
      expect(await screen.findByRole("tree", { name: "Databases" })).toBeInTheDocument();
      await act(() => runCommand("view.sidebar"));
      expect(selectSidebarHidden(useStore.getState())).toBe(true);
      expect(tree()).not.toBeInTheDocument();
      await waitFor(() => expect(mocked.setUiState).toHaveBeenCalledWith(session.identity, expect.objectContaining({ sidebarHidden: true })));
    });

    it("toggles the sidebar on ⌘\\", async () => {
      render(<App />);
      await screen.findByRole("tree", { name: "Databases" });
      fireEvent.keyDown(window, { key: "\\", code: "Backslash", metaKey: true });
      expect(tree()).not.toBeInTheDocument();
      fireEvent.keyDown(window, { key: "\\", code: "Backslash", metaKey: true });
      expect(await screen.findByRole("tree", { name: "Databases" })).toBeInTheDocument();
    });

    it("hides the sidebar when Toggle sidebar is chosen in the palette", async () => {
      render(<App />);
      await screen.findByRole("tree", { name: "Databases" });
      act(() => useStore.getState().setPaletteOpen(true));
      await userEvent.type(await screen.findByLabelText("Search everything"), "Toggle sidebar");
      await userEvent.click(await screen.findByRole("option", { name: /Toggle sidebar/ }));
      expect(tree()).not.toBeInTheDocument();
    });

    it("hides the sidebar from the status bar button", async () => {
      render(<App />);
      const toggle = await screen.findByRole("button", { name: "Toggle sidebar" });
      expect(toggle).toHaveAttribute("aria-pressed", "true");
      await userEvent.click(toggle);
      expect(toggle).toHaveAttribute("aria-pressed", "false");
      expect(tree()).not.toBeInTheDocument();
    });

    it("keeps the sidebar hidden after reconnecting when the saved view state says so", async () => {
      resetStore();
      mocked.connect.mockResolvedValue(session);
      mocked.getUiState.mockResolvedValueOnce({ ...EMPTY_UI_STATE, sidebarHidden: true });
      render(<App />);
      await act(() => useStore.getState().connect({ spec: session.spec }));
      await waitFor(() => expect(tree()).not.toBeInTheDocument());
      expect(await screen.findByRole("button", { name: "Toggle sidebar" })).toHaveAttribute("aria-pressed", "false");
    });
  });

  describe("shortcuts behind overlays", () => {
    const cmdW = { key: "w", code: "KeyW", metaKey: true };

    beforeEach(() => {
      vi.clearAllMocks();
      mocked.execute.mockResolvedValue({ results: [resultSet()], elapsedMs: 3, database: "db", failure: null });
    });

    describe("with Settings open", () => {
      beforeEach(() => {
        connectedStore();
        useStore.getState().openDialog({ type: "settings" });
      });

      it("keeps the tab when ⌘W is pressed", async () => {
        render(<App />);
        await screen.findByRole("heading", { name: "Appearance" });
        fireEvent.keyDown(window, cmdW);
        expect(screen.getByRole("tab", { name: /Query 1/ })).toBeInTheDocument();
        expect(screen.getByRole("heading", { name: "Appearance" })).toBeInTheDocument();
      });

      it("closes Settings on Escape", async () => {
        render(<App />);
        await screen.findByRole("heading", { name: "Appearance" });
        await userEvent.keyboard("{Escape}");
        expect(screen.queryByRole("heading", { name: "Appearance" })).not.toBeInTheDocument();
      });

      it("closes Settings on ⌘, and reopens it on the next press", async () => {
        render(<App />);
        await screen.findByRole("heading", { name: "Appearance" });
        fireEvent.keyDown(window, { key: ",", code: "Comma", metaKey: true });
        expect(screen.queryByRole("heading", { name: "Appearance" })).not.toBeInTheDocument();
        fireEvent.keyDown(window, { key: ",", code: "Comma", metaKey: true });
        expect(await screen.findByRole("heading", { name: "Appearance" })).toBeInTheDocument();
      });
    });

    describe("with the palette open", () => {
      const openPalette = async () => {
        render(<App />);
        await screen.findByText("wp_options");
        act(() => useStore.getState().setPaletteOpen(true));
        await screen.findByRole("dialog", { name: "Command palette" });
      };
      const cmdEnter = { key: "Enter", code: "Enter", metaKey: true };

      beforeEach(() => connectedStore({ tabs: [queryTab({ sql: "SELECT 1" })] }));

      it("runs the statement on ⌘↵ when no overlay is open", async () => {
        render(<App />);
        await screen.findByText("wp_options");
        fireEvent.keyDown(window, cmdEnter);
        expect(await screen.findByText("Hello world")).toBeInTheDocument();
      });

      it("does not run the statement on ⌘↵", async () => {
        await openPalette();
        fireEvent.keyDown(window, cmdEnter);
        expect(mocked.execute).not.toHaveBeenCalled();
        expect(screen.queryByText("Hello world")).not.toBeInTheDocument();
      });
    });

    describe("auto-repeat", () => {
      beforeEach(() => connectedStore({ tabs: [queryTab({ id: "q1" }), queryTab({ id: "q2", title: "Query 2" })] }));

      it("closes no tab for a repeated ⌘W", async () => {
        render(<App />);
        await screen.findByText("wp_options");
        fireEvent.keyDown(window, { ...cmdW, repeat: true });
        expect(screen.getAllByRole("tab")).toHaveLength(2);
      });

      it("closes exactly one tab when a ⌘W is followed by repeated ones", async () => {
        render(<App />);
        await screen.findByText("wp_options");
        fireEvent.keyDown(window, cmdW);
        fireEvent.keyDown(window, { ...cmdW, repeat: true });
        fireEvent.keyDown(window, { ...cmdW, repeat: true });
        expect(screen.getAllByRole("tab")).toHaveLength(1);
      });
    });
  });

  describe("database accent", () => {
    const root = document.documentElement;
    const blog: SessionInfo = { ...session, id: "s2", identity: "root@127.0.0.1:52958/blog", displayName: "Blog", database: "blog", slug: "blog" };
    const accentIs = (accent: string) => waitFor(() => expect(root.dataset.accent).toBe(accent));

    beforeEach(() => {
      vi.clearAllMocks();
      connectedStore();
      mocked.projectAccents.mockResolvedValue({ "shop/db": "blue", "shop/shop2": "gold", "blog/blog": "purple" });
    });

    it("sets the open database's accent on the page instead of the app's", async () => {
      render(<App />);
      await accentIs("blue");
    });

    it("keeps the app's accent for a database without its own", async () => {
      mocked.projectAccents.mockResolvedValue({ "shop/other": "blue" });
      render(<App />);
      await screen.findByText("wp_options");
      expect(root.dataset.accent).toBe("orange");
    });

    it("changes at once when the database is switched", async () => {
      render(<App />);
      await accentIs("blue");
      await act(() => useStore.getState().switchDatabase("s1", "shop2"));
      await accentIs("gold");
    });

    it("changes at once when another session is activated", async () => {
      useStore.setState({ sessions: [session, blog] });
      render(<App />);
      await accentIs("blue");
      act(() => useStore.getState().setActiveSession("s2"));
      await accentIs("purple");
    });

    it("changes at once when the accent is set and falls back to the app's when cleared", async () => {
      render(<App />);
      await accentIs("blue");
      await act(() => useStore.getState().setAccent("s1", "db", "graphite"));
      await accentIs("graphite");
      await act(() => useStore.getState().setAccent("s1", "db", null));
      await accentIs("orange");
    });

    it("returns to the app's accent after the session is disconnected", async () => {
      render(<App />);
      await accentIs("blue");
      await act(() => useStore.getState().disconnect("s1"));
      await accentIs("orange");
    });

    it("is shared by two sessions of the same connection and database", async () => {
      useStore.setState({ sessions: [session, { ...session, id: "s3" }] });
      render(<App />);
      await accentIs("blue");
      act(() => useStore.getState().setActiveSession("s3"));
      expect(root.dataset.accent).toBe("blue");
    });
  });

  describe("palette commands across sessions", () => {
    const planned = { id: 1, description: "Set post_title", statements: ["UPDATE wp_posts SET post_title = 'x'"], canUndo: true };
    const other: SessionInfo = { ...session, id: "s2", identity: "root@127.0.0.1:52958/blog", displayName: "Blog", database: "db", slug: "blog" };

    const search = async (text: string) => {
      act(() => useStore.getState().setPaletteOpen(true));
      await userEvent.type(await screen.findByLabelText("Search everything"), text);
    };
    const switchTo = async (id: string) => {
      await userEvent.keyboard("{Escape}");
      act(() => useStore.getState().setActiveSession(id));
    };

    beforeEach(() => {
      vi.clearAllMocks();
      connectedStore();
      useStore.setState({ sessions: [session, other] });
    });

    it("lists Apply pending changes only for the session that has pending changes", async () => {
      useStore.setState({ pending: [{ ...planned, id: 1, sessionId: "s2" }] });
      render(<App />);
      await search("Apply pending");
      expect(screen.queryByRole("option", { name: /Apply pending changes/ })).not.toBeInTheDocument();

      await switchTo("s2");
      await search("Apply pending");
      expect(await screen.findByRole("option", { name: /Apply pending changes/ })).toBeInTheDocument();
    });

    it("lists Undo last apply only for the session that has an applied batch", async () => {
      useStore.setState({ applied: [{ sessionId: "s2", batchId: 1, canUndo: true }] });
      render(<App />);
      await search("Undo last");
      expect(screen.queryByRole("option", { name: /Undo last apply/ })).not.toBeInTheDocument();

      await switchTo("s2");
      await search("Undo last");
      expect(await screen.findByRole("option", { name: /Undo last apply/ })).toBeInTheDocument();
    });
  });
});
