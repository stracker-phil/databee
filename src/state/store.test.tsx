import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import App from "../App";
import { api, type ExecuteResponse, type SessionInfo } from "../api";
import { installCommands, runCommand } from "../commands";
import { StatusBar } from "../components/StatusBar";
import { TabBar } from "../components/Tabs";
import { Toasts } from "../components/Toasts";
import { Toolbar } from "../components/Toolbar";
import { columnMeta, connectedStore, pendingChange, queryTab, resultSet, session, text } from "../test/fixtures";
import { objectDocument } from "../test/objectFixtures";
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
        useStore.setState({
          pending: [pendingChange({ id: 1, sessionId: "s1" }), pendingChange({ id: 2, sessionId: "s1" }), pendingChange({ id: 3, sessionId: "s2" })],
          applied: [
            { sessionId: "s1", batchId: 10, canUndo: true },
            { sessionId: "s2", batchId: 11, canUndo: true },
          ],
          documents: {
            [documentKey("s1", "order 4711")]: { version: 0, document: objectDocument(), error: null },
            [documentKey("s2", "order 4711")]: { version: 0, document: objectDocument(), error: null },
          },
        });
      });

      it("removes the closed active session's changes, cell and documents and keeps the other session's", async () => {
        render(
          <>
            <StatusBar />
            <TabBar />
            <Toolbar />
          </>,
        );
        expect(screen.getByRole("button", { name: "2 pending changes" })).toBeInTheDocument();

        await act(() => useStore.getState().disconnect("s1"));

        expect(screen.getByRole("button", { name: "1 pending change" })).toBeInTheDocument();
        expect(tabTitles()).toEqual(["Blog scratch"]);
        const state = useStore.getState();
        expect(state.pending.map((p) => p.sessionId)).toEqual(["s2"]);
        expect(state.applied.map((a) => a.sessionId)).toEqual(["s2"]);
        expect(state.selectedCell).toBeNull();
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

      expect(useStore.getState().tabs.filter((t) => t.kind === "query" && t.sessionId === "s1")).toHaveLength(1);
      expect(useStore.getState().sessions).toHaveLength(1);
    });
  });
});
