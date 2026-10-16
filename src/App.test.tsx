import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", async (orig) => ({ ...(await orig<object>()), api: (await import("./test/apiMock")).createApiMock() }));

import App from "./App";
import { api, type SessionInfo } from "./api";
import { runCommand } from "./commands";
import { resetStore, useStore } from "./state/store";
import { connectedStore, queryTab, resultSet, session, settings } from "./test/fixtures";

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
    expect(screen.getByRole("button", { name: /root@127.0.0.1:52958\/db/ })).toHaveAttribute("title", expect.stringContaining("10.3.39-MariaDB"));
  });

  it("runs menu commands by id", async () => {
    connectedStore();
    render(<App />);
    await screen.findByText("wp_options");
    act(() => runCommand("tabs.new"));
    expect(screen.getAllByRole("tab")).toHaveLength(2);
    act(() => runCommand("tabs.close"));
    expect(screen.getAllByRole("tab")).toHaveLength(1);
    act(() => runCommand("settings.open"));
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

  it("keeps the same editor, with its text, when selecting a cell opens the bottom inspector", async () => {
    vi.clearAllMocks();
    const bottom = settings({ inspectorDock: "bottom" });
    mocked.getSettings.mockResolvedValueOnce(bottom);
    mocked.execute.mockResolvedValue({ results: [resultSet()], elapsedMs: 3, database: "db" });
    connectedStore({ tabs: [queryTab({ sql: "SELECT ID, post_title FROM wp_posts" })], settings: bottom });
    const { container } = render(<App />);
    await screen.findByText("wp_options");
    const editor = container.querySelector(".cm-content") as HTMLElement;
    expect(editor).toHaveTextContent("SELECT ID, post_title FROM wp_posts");

    await userEvent.click(screen.getByRole("button", { name: /^Run$/ }));
    await userEvent.click(await screen.findByText("Hello world"));

    expect(screen.getByRole("complementary", { name: "Value inspector" })).toBeInTheDocument();
    expect(editor).toBeInTheDocument();
    expect(container.querySelector(".cm-content")).toBe(editor);
    expect(editor).toHaveTextContent("SELECT ID, post_title FROM wp_posts");
  });

  it("toasts the message of a failed launch request instead of an object", async () => {
    vi.clearAllMocks();
    mocked.takeLaunchEvents.mockRejectedValueOnce({ kind: "io", message: "socket closed", code: null });
    render(<App />);
    expect(await screen.findByText("Launch request failed: socket closed")).toBeInTheDocument();
    expect(screen.queryByText(/\[object Object\]/)).not.toBeInTheDocument();
  });

  describe("shortcuts behind overlays", () => {
    const cmdW = { key: "w", code: "KeyW", metaKey: true };

    beforeEach(() => {
      vi.clearAllMocks();
      mocked.execute.mockResolvedValue({ results: [resultSet()], elapsedMs: 3, database: "db" });
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

  describe("palette commands across sessions", () => {
    const planned = { description: "Set post_title", statements: ["UPDATE wp_posts SET post_title = 'x'"], undo: ["UPDATE wp_posts SET post_title = 'y'"] };
    const other: SessionInfo = { ...session, id: "s2", identity: "root@127.0.0.1:52958/blog", displayName: "Blog", database: "db" };

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
      useStore.setState({ applied: [{ sessionId: "s2", changes: [planned] }] });
      render(<App />);
      await search("Undo last");
      expect(screen.queryByRole("option", { name: /Undo last apply/ })).not.toBeInTheDocument();

      await switchTo("s2");
      await search("Undo last");
      expect(await screen.findByRole("option", { name: /Undo last apply/ })).toBeInTheDocument();
    });
  });
});
