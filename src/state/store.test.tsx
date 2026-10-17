import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import App from "../App";
import { api, type ExecuteResponse, type SessionInfo, type WpInfo } from "../api";
import { installCommands, runCommand } from "../commands";
import { StatusBar } from "../components/StatusBar";
import { TabBar } from "../components/Tabs";
import { Toasts } from "../components/Toasts";
import { Toolbar } from "../components/Toolbar";
import { columnMeta, connectedStore, pendingChange, queryTab, resultSet, session, text } from "../test/fixtures";
import { orderDocument, wpInfo, wpStore } from "../test/wpFixtures";
import { documentKey } from "./slices/objects";
import { useStore, type QueryTab } from "./store";

const mocked = vi.mocked(api, true);

const second: SessionInfo = {
  ...session,
  id: "s2",
  spec: { ...session.spec, name: "Blog", port: 52959 },
  identity: "root@127.0.0.1:52959/db",
  displayName: "Blog",
  slug: "blog",
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

/** `connectedStore` plus a second session, with `tabs` replacing the fixture tab. */
function twoSessions(tabs: QueryTab[], active: { session: string; tab: string }) {
  connectedStore({ tabs });
  useStore.setState((s) => ({
    sessions: [session, second],
    activeSessionId: active.session,
    activeTabId: active.tab,
    schema: { ...s.schema, s2: { databases: [], tables: { db: [] } } },
  }));
}

const inspect = (tabId: string) =>
  useStore.setState({ selectedCell: { tabId, row: 0, col: 0, column: columnMeta("option_value", "LONGTEXT"), cell: text("hello"), source: { sessionId: "s1", resultId: 1 } } });

const tabTitles = () => screen.getAllByRole("tab").map((t) => t.querySelector(".title")?.textContent);

const planned = (description: string) => ({ description, statements: [`-- ${description}`], canUndo: true });

describe("store behavior", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocked.wpDetect.mockReset().mockResolvedValue(null);
    mocked.execute.mockReset();
    connectedStore();
  });

  describe("disconnect", () => {
    it("drops the pending changes and tabs of that session and keeps the other session's", async () => {
      twoSessions([queryTab({ id: "a", title: "Shop scratch" }), queryTab({ id: "b", sessionId: "s2", title: "Blog scratch" })], { session: "s1", tab: "a" });
      useStore.setState({
        pending: [
          { ...planned("shop edit"), id: 1, sessionId: "s1" },
          { ...planned("blog edit"), id: 2, sessionId: "s2" },
          { ...planned("blog edit 2"), id: 3, sessionId: "s2" },
        ],
      });
      render(<App />);
      const tray = () => within(screen.getByRole("region", { name: "Pending changes" }));
      expect(tray().getByText("1 pending change")).toBeInTheDocument();
      expect(tabTitles()).toEqual(["Shop scratch"]);

      await act(() => useStore.getState().disconnect("s1"));

      expect(tray().getByText("2 pending changes")).toBeInTheDocument();
      expect(tabTitles()).toEqual(["Blog scratch"]);
      expect(useStore.getState().pending.map((p) => p.sessionId)).toEqual(["s2", "s2"]);
      expect(useStore.getState().tabs.map((t) => t.id)).toEqual(["b"]);
    });

    describe("with state in every slice", () => {
      beforeEach(() => {
        twoSessions([queryTab({ id: "a", title: "Shop scratch" }), queryTab({ id: "b", sessionId: "s2", title: "Blog scratch" })], { session: "s1", tab: "a" });
        inspect("a");
        useStore.setState((s) => ({
          pending: [pendingChange({ id: 1, sessionId: "s1" }), pendingChange({ id: 2, sessionId: "s1" }), pendingChange({ id: 3, sessionId: "s2" })],
          applied: [
            { sessionId: "s1", batchId: 10, canUndo: true },
            { sessionId: "s2", batchId: 11, canUndo: true },
          ],
          wp: { ...s.wp, "s1/db": wpInfo, "s2/other": wpInfo },
          wpNav: { stack: [{ screen: "dashboard" }, { screen: "snapshots" }], index: 1 },
          documents: {
            [documentKey("s1", "order 4711")]: { version: 0, document: orderDocument(), error: null },
            [documentKey("s2", "order 4711")]: { version: 0, document: orderDocument(), error: null },
          },
        }));
      });

      it("removes the closed active session's changes, cell, WordPress info and navigation and keeps the other session's", async () => {
        render(
          <>
            <StatusBar />
            <TabBar />
            <Toolbar />
          </>,
        );
        expect(screen.getByRole("button", { name: "2 pending changes" })).toBeInTheDocument();
        expect(screen.getByRole("group", { name: "Mode" })).toBeInTheDocument();

        await act(() => useStore.getState().disconnect("s1"));

        expect(screen.getByRole("button", { name: "1 pending change" })).toBeInTheDocument();
        expect(tabTitles()).toEqual(["Blog scratch"]);
        expect(screen.queryByRole("group", { name: "Mode" })).not.toBeInTheDocument();
        const state = useStore.getState();
        expect(state.pending.map((p) => p.sessionId)).toEqual(["s2"]);
        expect(state.applied.map((a) => a.sessionId)).toEqual(["s2"]);
        expect(state.selectedCell).toBeNull();
        expect(Object.keys(state.wp)).toEqual(["s2/other"]);
        expect(state.wpNav).toEqual({ stack: [{ screen: "dashboard" }], index: 0 });
        expect(Object.keys(state.documents)).toEqual([documentKey("s2", "order 4711")]);
      });

      it("leaves the active session's changes, cell and navigation alone when another session closes", async () => {
        render(
          <>
            <StatusBar />
            <TabBar />
          </>,
        );

        await act(() => useStore.getState().disconnect("s2"));

        expect(screen.getByRole("button", { name: "2 pending changes" })).toBeInTheDocument();
        expect(tabTitles()).toEqual(["Shop scratch"]);
        const state = useStore.getState();
        expect(state.applied.map((a) => a.sessionId)).toEqual(["s1"]);
        expect(state.selectedCell?.tabId).toBe("a");
        expect(Object.keys(state.wp)).toEqual(["s1/db"]);
        expect(state.wpNav.index).toBe(1);
        expect(Object.keys(state.documents)).toEqual([documentKey("s1", "order 4711")]);
      });
    });

    it("clears the selected cell when its tab belonged to the session", async () => {
      twoSessions([queryTab({ id: "a" }), queryTab({ id: "b", sessionId: "s2", title: "Blog scratch" })], { session: "s1", tab: "a" });
      inspect("a");
      expect(useStore.getState().selectedCell?.tabId).toBe("a");

      await act(() => useStore.getState().disconnect("s1"));

      expect(useStore.getState().selectedCell).toBeNull();
    });
  });

  describe("loadWp", () => {
    it("switches to WordPress mode when the active database is detected as WordPress", async () => {
      await useStore.getState().loadWp("s1", "db");
      expect(useStore.getState().mode).toBe("db");

      mocked.wpDetect.mockResolvedValue(wpInfo);
      await useStore.getState().loadWp("s1", "db");
      expect(useStore.getState().mode).toBe("wp");
    });

    it("stays in Database mode when the user switched to a non-WordPress database before the detection of the old one replied", async () => {
      const a = deferred<WpInfo | null>();
      mocked.wpDetect.mockImplementation((_session, database) => (database === "A" ? a.promise : Promise.resolve(null)));
      useStore.setState({ sessions: [{ ...session, database: "A" }] });

      const loading = useStore.getState().loadWp("s1", "A");
      await act(() => useStore.getState().switchDatabase("s1", "B"));
      a.resolve(wpInfo);
      await act(() => loading);

      expect(useStore.getState().mode).toBe("db");
    });

    it("stores only the reply of the later request for the same database", async () => {
      const first = deferred<WpInfo | null>();
      const later = deferred<WpInfo | null>();
      mocked.wpDetect.mockReturnValueOnce(first.promise).mockReturnValueOnce(later.promise);

      const a = useStore.getState().loadWp("s1", "db");
      const b = useStore.getState().loadWp("s1", "db", true);
      later.resolve({ ...wpInfo, prefix: "later_" });
      await b;
      first.resolve({ ...wpInfo, prefix: "first_" });
      await a;

      expect(useStore.getState().wp["s1/db"]?.prefix).toBe("later_");
    });

    it("toasts once that the legacy entities file is no longer read, however often detection runs", async () => {
      mocked.wpDetect.mockResolvedValue({ ...wpInfo, legacyEntitiesFile: "/config/entities.toml" });
      render(<Toasts />);

      await act(() => useStore.getState().loadWp("s1", "db"));
      await act(() => useStore.getState().loadWp("s1", "db", true));

      expect(await screen.findAllByText(/\/config\/entities\.toml is no longer read/)).toHaveLength(1);
    });

    it("does not toast without a legacy entities file", async () => {
      mocked.wpDetect.mockResolvedValue(wpInfo);
      render(<Toasts />);
      await act(() => useStore.getState().loadWp("s1", "db"));
      expect(screen.queryByText(/no longer read/)).not.toBeInTheDocument();
    });
  });

  describe("runQuery", () => {
    it("ignores a second run while the tab is running and shows the first result", async () => {
      connectedStore({ tabs: [queryTab({ sql: "SELECT 1" })] });
      const call = deferred<ExecuteResponse>();
      mocked.execute.mockReturnValue(call.promise);
      render(<App />);

      await act(async () => {
        const runs = [useStore.getState().runQuery("q1", "SELECT 1"), useStore.getState().runQuery("q1", "SELECT 2")];
        call.resolve({ results: [resultSet()], elapsedMs: 3, database: "db", failure: null });
        await Promise.all(runs);
      });

      expect(await screen.findByText("Hello world")).toBeInTheDocument();
      expect(mocked.execute).toHaveBeenCalledTimes(1);
      expect(mocked.execute).toHaveBeenCalledWith("s1", "SELECT 1", false);
    });
  });

  describe("closeTab", () => {
    const tabs = () => [queryTab({ id: "a", title: "Alpha" }), queryTab({ id: "b", sessionId: "s2", title: "Beta" }), queryTab({ id: "c", sessionId: "s2", title: "Gamma" })];

    it("activates the neighbouring tab of the same session when the active tab closes", async () => {
      twoSessions(tabs(), { session: "s2", tab: "c" });
      render(<TabBar />);

      await userEvent.click(screen.getByRole("button", { name: "Close Gamma" }));

      expect(screen.getByRole("tab", { name: /Beta/ })).toHaveAttribute("aria-selected", "true");
    });

    it("keeps the selected cell of the active tab when another tab closes", async () => {
      twoSessions(tabs(), { session: "s2", tab: "c" });
      inspect("c");
      render(<App />);

      await userEvent.click(screen.getByRole("button", { name: "Close Beta" }));

      expect(screen.queryByRole("tab", { name: /Beta/ })).not.toBeInTheDocument();
      expect(useStore.getState().selectedCell?.tabId).toBe("c");
    });
  });

  describe("query tab titles", () => {
    it("never reuses a number of a closed query tab", async () => {
      render(<TabBar />);
      const newTab = screen.getByRole("button", { name: "New query tab" });

      await userEvent.click(newTab);
      expect(tabTitles()).toEqual(["Query 1", "Query 2"]);
      await userEvent.click(screen.getByRole("button", { name: "Close Query 2" }));
      await userEvent.click(newTab);

      expect(tabTitles()).toEqual(["Query 1", "Query 3"]);
    });

    it("counts per session", async () => {
      useStore.setState((s) => ({ sessions: [session, second], activeSessionId: "s2", schema: { ...s.schema, s2: { databases: [], tables: {} } } }));
      render(<TabBar />);

      await userEvent.click(screen.getByRole("button", { name: "New query tab" }));

      expect(tabTitles()).toEqual(["Query 1"]);
    });
  });

  describe("switchDatabase", () => {
    it("shows Database mode after switching from WordPress mode to a database that is not WordPress", async () => {
      wpStore();
      useStore.setState((s) => ({ wp: { ...s.wp, "s1/plain": null } }));

      await useStore.getState().switchDatabase("s1", "plain");
      render(<App />);

      expect(useStore.getState().mode).toBe("db");
      expect(screen.getByRole("tablist")).toBeInTheDocument();
    });

    it("keeps WordPress mode when switching to a known WordPress database", async () => {
      wpStore();
      useStore.setState((s) => ({ wp: { ...s.wp, "s1/other": wpInfo } }));

      await useStore.getState().switchDatabase("s1", "other");

      expect(useStore.getState().mode).toBe("wp");
    });

  });

  describe("navigate", () => {
    it("adds no history entry for the same route with a different key order", () => {
      const { navigate } = useStore.getState();
      navigate({ screen: "snapshots" });
      navigate({ screen: "snapshots" });

      expect(useStore.getState().wpNav.stack).toHaveLength(2);
    });
  });

  describe("view.refresh", () => {
    beforeEach(() => {
      mocked.refreshSchema.mockReset().mockResolvedValue(undefined);
      installCommands();
    });

    it("forgets the active session's cached schema, then reloads every view", async () => {
      const before = useStore.getState().dataVersion;
      let versionAtCall: number | undefined;
      mocked.refreshSchema.mockImplementation(async () => {
        versionAtCall = useStore.getState().dataVersion;
      });

      await act(async () => {
        runCommand("view.refresh");
      });

      expect(mocked.refreshSchema).toHaveBeenCalledWith("s1");
      expect(versionAtCall).toBe(before);
      expect(useStore.getState().dataVersion).toBe(before + 1);
    });

    it("still reloads the views and shows an error toast when the schema refresh is rejected", async () => {
      mocked.refreshSchema.mockRejectedValue({ kind: "io", message: "connection lost", code: null });
      const before = useStore.getState().dataVersion;
      render(<Toasts />);

      await act(() => useStore.getState().refreshView());

      expect(await screen.findByRole("alert")).toHaveTextContent("Could not refresh the schema: connection lost");
      expect(useStore.getState().dataVersion).toBe(before + 1);
    });

    it("reloads the views without a schema call when no session is active", async () => {
      useStore.setState({ sessions: [], activeSessionId: null });
      const before = useStore.getState().dataVersion;

      await act(() => useStore.getState().refreshView());

      expect(mocked.refreshSchema).not.toHaveBeenCalled();
      expect(useStore.getState().dataVersion).toBe(before + 1);
    });
  });

  describe("connect", () => {
    it("opens no second query tab for a session that is already open", async () => {
      mocked.connect.mockResolvedValue(session);

      await act(() => useStore.getState().connect({ spec: session.spec }));

      expect(useStore.getState().tabs.filter((t) => t.sessionId === "s1")).toHaveLength(1);
      expect(useStore.getState().sessions).toHaveLength(1);
    });
  });
});
