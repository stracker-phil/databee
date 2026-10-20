import { act } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api, type Address, type SessionInfo } from "../../api";
import { installCommands, runCommand } from "../../commands";
import { address } from "../../lib/address";
import { documentText } from "../../lib/document";
import { connectedStore, pendingChange, queryTab, readoutOf, session, tableTab } from "../../test/fixtures";
import { activeObject, objectDocument, orderAddress } from "../../test/objectFixtures";
import { fail } from "../requests";
import { documentKey } from "./objects";
import { readoutKey } from "./readout";
import { selectActiveTab, useStore, type LostHealth } from "../store";

const mocked = vi.mocked(api, true);
const store = () => useStore.getState();

const second: SessionInfo = {
  ...session,
  id: "s2",
  spec: { ...session.spec, name: "Blog", port: 52959 },
  identity: "root@127.0.0.1:52959/db",
  displayName: "Blog",
  slug: "blog",
};

const down = (error = "server has gone away"): LostHealth => ({ state: "down", since: 1_700_000_000_000, error, retryAt: 1_700_000_030_000 });
const reconnecting: LostHealth = { state: "reconnecting", since: 1_700_000_000_000, error: "server has gone away" };

const toasts = () => store().toasts.map(({ kind, message, count }) => ({ kind, message, count }));
const lost = (...ids: string[]) => useStore.setState({ health: Object.fromEntries(ids.map((id) => [id, down()])) });
const notice = (what: string) => ({ kind: "info", message: `Connection lost: can't ${what} until it is back`, count: 1 });

const rowOf = (table: string, key: string): Address => address("shop", "db", { kind: "row", table, key });
const order4712 = rowOf("wp_wc_orders", "4712");
const rowsView: Address = { ...orderAddress, view: "rows" };

function loaded(row: Address) {
  store().keepDocument("s1", objectDocument({ address: row }));
}

describe("connection health", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [queryTab({ sql: "SELECT 1" })] });
    installCommands();
  });

  describe("setHealth", () => {
    it("records a session that went down and toasts the loss once", () => {
      store().setHealth({ sessionId: "s1", health: down() });

      expect(store().health).toEqual({ s1: down() });
      expect(toasts()).toEqual([{ kind: "info", message: "Lost the connection to Shop", count: 1 }]);
    });

    it("toasts nothing more when an already lost session reports down or reconnecting", () => {
      store().setHealth({ sessionId: "s1", health: down() });
      store().setHealth({ sessionId: "s1", health: reconnecting });
      store().setHealth({ sessionId: "s1", health: down("still gone") });

      expect(store().health.s1).toEqual(down("still gone"));
      expect(toasts()).toHaveLength(1);
      expect(toasts()[0].count).toBe(1);
    });

    it("removes the entry, toasts the return and refreshes the view when the active session is back", async () => {
      store().setHealth({ sessionId: "s1", health: down() });
      const before = store().dataVersion;

      store().setHealth({ sessionId: "s1", health: { state: "ok" } });

      expect(store().health).toEqual({});
      expect(toasts().at(-1)).toEqual({ kind: "success", message: "Reconnected to Shop", count: 1 });
      await vi.waitFor(() => expect(store().dataVersion).toBe(before + 1));
      expect(mocked.refreshSchema).toHaveBeenCalledWith("s1");
    });

    it("does not refresh the view when a session in the background is back", async () => {
      useStore.setState({ sessions: [session, second] });
      store().setHealth({ sessionId: "s2", health: down() });
      const before = store().dataVersion;

      await act(async () => store().setHealth({ sessionId: "s2", health: { state: "ok" } }));

      expect(toasts().at(-1)).toEqual({ kind: "success", message: "Reconnected to Blog", count: 1 });
      expect(mocked.refreshSchema).not.toHaveBeenCalled();
      expect(store().dataVersion).toBe(before);
    });

    it("ignores an ok event for a session that was not lost", () => {
      store().setHealth({ sessionId: "s1", health: { state: "ok" } });

      expect(toasts()).toEqual([]);
      expect(mocked.refreshSchema).not.toHaveBeenCalled();
    });

    it.each([down(), { state: "ok" } as const])("ignores an event ($state) for a session that is not open", (health) => {
      store().setHealth({ sessionId: "gone", health });

      expect(store().health).toEqual({});
      expect(toasts()).toEqual([]);
    });
  });

  describe("retryConnection", () => {
    beforeEach(() => {
      lost("s1");
    });

    it("asks Rust to reconnect and clears the entry when the reply is ok, since Rust sends no event then", async () => {
      mocked.reconnectSession.mockResolvedValue({ state: "ok" });

      await act(() => store().retryConnection("s1"));

      expect(mocked.reconnectSession).toHaveBeenCalledWith("s1");
      expect(store().health).toEqual({});
      expect(toasts()).toEqual([{ kind: "success", message: "Reconnected to Shop", count: 1 }]);
      await vi.waitFor(() => expect(mocked.refreshSchema).toHaveBeenCalledWith("s1"));
    });

    it("leaves the recorded health to the events when the reply is reconnecting", async () => {
      mocked.reconnectSession.mockResolvedValue(reconnecting);

      await store().retryConnection("s1");

      expect(store().health).toEqual({ s1: down() });
      expect(toasts()).toEqual([]);
    });

    it("says why there is still no connection when the server does not answer", async () => {
      mocked.reconnectSession.mockResolvedValue(down("connection refused"));

      await store().retryConnection("s1");

      expect(toasts()).toEqual([{ kind: "info", message: "Still no connection to Shop: connection refused", count: 1 }]);
      expect(store().health).toEqual({ s1: down() });
    });

    it("toasts nothing when a quiet retry finds the server still down", async () => {
      mocked.reconnectSession.mockResolvedValue(down("connection refused"));

      await store().retryConnection("s1", true);

      expect(mocked.reconnectSession).toHaveBeenCalledWith("s1");
      expect(toasts()).toEqual([]);
    });

    it("toasts the error when the retry call is rejected, unless it is quiet", async () => {
      mocked.reconnectSession.mockRejectedValue({ kind: "session_not_found", message: "No such session", code: null });

      await store().retryConnection("s1", true);
      expect(toasts()).toEqual([]);

      await store().retryConnection("s1");
      expect(toasts()).toEqual([{ kind: "error", message: "Retry failed: No such session", count: 1 }]);
    });
  });

  describe("retryLostSessions", () => {
    it("retries every down session quietly and skips the ones already reconnecting", async () => {
      useStore.setState({ sessions: [session, second, { ...second, id: "s3", slug: "docs", displayName: "Docs" }] });
      useStore.setState({ health: { s1: down(), s2: reconnecting, s3: down() } });
      mocked.reconnectSession.mockResolvedValue(down("connection refused"));

      await act(async () => store().retryLostSessions());

      expect(mocked.reconnectSession.mock.calls).toEqual([["s1"], ["s3"]]);
      expect(toasts()).toEqual([]);
    });

    it("calls nothing when every session is fine", () => {
      store().retryLostSessions();

      expect(mocked.reconnectSession).not.toHaveBeenCalled();
    });
  });

  describe("disconnect", () => {
    it("drops the health entry of the closed session and keeps the other's", async () => {
      useStore.setState((s) => ({
        sessions: [session, second],
        schema: { ...s.schema, s2: { databases: [], tables: { db: [] } } },
        health: { s1: down(), s2: down() },
      }));

      await act(() => store().disconnect("s1"));

      expect(Object.keys(store().health)).toEqual(["s2"]);
    });
  });

  describe("connection.retry", () => {
    it("is not runnable while the active session is fine or already reconnecting", () => {
      expect(runCommand("connection.retry")).toBe(false);

      useStore.setState({ health: { s1: reconnecting } });
      expect(runCommand("connection.retry")).toBe(false);
      expect(mocked.reconnectSession).not.toHaveBeenCalled();
    });

    it("is not runnable when only a background session is down", () => {
      useStore.setState({ sessions: [session, second], health: { s2: down() } });

      expect(runCommand("connection.retry")).toBe(false);
    });

    it("reconnects the active session when it is down", () => {
      lost("s1");

      expect(runCommand("connection.retry")).toBe(true);
      expect(mocked.reconnectSession).toHaveBeenCalledWith("s1");
    });
  });

  describe("fail", () => {
    it("turns a connection_lost error into a short info toast", () => {
      const err = fail({ kind: "connection_lost", message: "Lost connection to MySQL server", code: null }, "Could not list databases");

      expect(err.kind).toBe("connection_lost");
      expect(toasts()).toEqual([{ kind: "info", message: "Could not list databases: connection lost", count: 1 }]);
    });

    it("keeps the message of other errors in an error toast", () => {
      fail({ kind: "database", message: "Table is locked", code: "1205" }, "Could not list databases");

      expect(toasts()).toEqual([{ kind: "error", message: "Could not list databases: Table is locked", count: 1 }]);
    });
  });

  describe("while the connection is lost", () => {
    describe("server work", () => {
      beforeEach(() => {
        lost("s1");
      });

      it("runs no query and says so", async () => {
        await store().runQuery("q1", "SELECT 1");

        expect(mocked.execute).not.toHaveBeenCalled();
        expect(toasts()).toEqual([notice("run queries")]);
      });

      it("queues no change", async () => {
        const id = await store().queueChange({ type: "deleteRow", database: "db", table: "wp_posts", keyColumn: "ID", key: "1" });

        expect(id).toBeNull();
        expect(mocked.planChange).not.toHaveBeenCalled();
        expect(toasts()).toEqual([notice("edit")]);
      });

      it("applies no pending change", async () => {
        useStore.setState({ pending: [pendingChange()] });

        await store().applyChanges();

        expect(mocked.applyChanges).not.toHaveBeenCalled();
        expect(store().pending).toHaveLength(1);
        expect(toasts()).toEqual([notice("apply changes")]);
      });

      it("undoes no applied batch", async () => {
        useStore.setState({ applied: [{ sessionId: "s1", batchId: 7, canUndo: true }] });

        await store().undoLast();

        expect(mocked.undoBatch).not.toHaveBeenCalled();
        expect(store().applied).toHaveLength(1);
        expect(toasts()).toEqual([notice("undo")]);
      });

      it("switches no database", async () => {
        await store().switchDatabase("s1", "other");

        expect(mocked.useDatabase).not.toHaveBeenCalled();
        expect(store().sessions[0].database).toBe("db");
        expect(toasts()).toEqual([notice("switch databases")]);
      });

      it("refreshes neither the schema nor the views", async () => {
        const before = store().dataVersion;

        await store().refreshView();

        expect(mocked.refreshSchema).not.toHaveBeenCalled();
        expect(store().dataVersion).toBe(before);
        expect(toasts()).toEqual([notice("refresh")]);
      });

      it("still runs queries of a session that is fine", async () => {
        useStore.setState({ sessions: [session, second], health: { s2: down() } });
        mocked.execute.mockResolvedValue({ results: [], elapsedMs: 1, database: "db", failure: null });

        await act(() => store().runQuery("q1", "SELECT 1"));

        expect(mocked.execute).toHaveBeenCalledWith("s1", "SELECT 1", false);
      });
    });

    describe("opening a table", () => {
      beforeEach(() => {
        connectedStore({ tabs: [queryTab(), tableTab()] });
        store().activateTab("q1");
        lost("s1");
      });

      it("opens no tab for a table that is not open", () => {
        store().openTableTab("db", "wp_options");

        expect(store().tabs.map((t) => t.id)).toEqual(["q1", "t1"]);
        expect(store().activeTabId).toBe("q1");
        expect(toasts()).toEqual([notice("open wp_options")]);
      });

      it("activates the tab of a table that is already open", () => {
        store().openTableTab("db", "wp_posts");

        expect(store().activeTabId).toBe("t1");
        expect(store().tabs).toHaveLength(2);
        expect(toasts()).toEqual([]);
      });
    });

    describe("opening an object", () => {
      beforeEach(() => {
        connectedStore({ tabs: [] });
        lost("s1");
      });

      it("opens a row whose document is loaded", () => {
        loaded(orderAddress);

        store().openObject(orderAddress);

        expect(activeObject().title).toBe("wp_wc_orders #4711");
        expect(toasts()).toEqual([]);
      });

      it("opens nothing for a row whose document is not loaded", () => {
        store().openObject(orderAddress);

        expect(store().tabs).toEqual([]);
        expect(toasts()).toEqual([notice("open wp_wc_orders #4711")]);
      });

      it("opens nothing for the Rows view of a row, even with its document loaded", () => {
        loaded(orderAddress);

        store().openObject(rowsView);

        expect(store().tabs).toEqual([]);
        expect(toasts()).toEqual([notice("open wp_wc_orders #4711")]);
      });

      it("shows the tab of a row that is already open", () => {
        useStore.setState({ health: {} });
        store().openObject(orderAddress);
        store().openObject(rowOf("wp_wc_orders", "1"));
        lost("s1");

        store().openObject(orderAddress);

        expect(activeObject().title).toBe("wp_wc_orders #4711");
        expect(store().tabs).toHaveLength(2);
      });

      it("opens no entity record", async () => {
        await store().openEntity("order", "4711");

        expect(mocked.document).not.toHaveBeenCalled();
        expect(store().tabs).toEqual([]);
        expect(toasts()).toEqual([notice("open order #4711")]);
      });

      describe("drilling from an open row", () => {
        beforeEach(() => {
          useStore.setState({ health: {} });
          store().openObject(orderAddress);
          lost("s1");
        });

        it("adds a column for a row whose document is loaded", () => {
          loaded(order4712);

          store().drill(activeObject().id, 0, order4712);

          expect(activeObject().columns).toHaveLength(2);
          expect(toasts()).toEqual([]);
        });

        it("adds no column for a row whose document is not loaded", () => {
          store().drill(activeObject().id, 0, order4712);

          expect(activeObject().columns).toHaveLength(1);
          expect(toasts()).toEqual([notice("open wp_wc_orders #4712")]);
        });

        it("adds no column for the Rows view of a row", () => {
          loaded(order4712);

          store().drill(activeObject().id, 0, { ...order4712, view: "rows" });

          expect(activeObject().columns).toHaveLength(1);
          expect(toasts()).toEqual([notice("open wp_wc_orders #4712")]);
        });
      });
    });

    describe("moving within a table tab", () => {
      const where = [{ column: "post_type", value: "page" }];
      const tableAt = (table: string): Address => address("shop", "db", { kind: "table", table, filter: [] });

      beforeEach(() => {
        connectedStore({ tabs: [tableTab()] });
        lost("s1");
      });

      const shown = () => {
        const tab = selectActiveTab(store());
        if (tab?.kind !== "table") throw new Error("the active tab is not a table tab");
        return tab;
      };

      it("shows no other rows", () => {
        store().showRows("t1", { database: "db", table: "wp_posts", where });

        expect(shown().where).toEqual([]);
        expect(toasts()).toEqual([notice("open wp_posts")]);
      });

      it("applies no typed filter", () => {
        store().setTableFilter("t1", "post_status = 'draft'");

        expect(shown().filter).toBeUndefined();
        expect(toasts()).toEqual([notice("filter rows")]);
      });

      it("moves to a row whose document is loaded and refuses one that is not", () => {
        store().navigateTab("t1", order4712);
        expect(selectActiveTab(store())?.kind).toBe("table");
        expect(toasts()).toEqual([notice("open wp_wc_orders #4712")]);

        loaded(order4712);
        store().navigateTab("t1", order4712);
        expect(selectActiveTab(store())?.kind).toBe("object");
      });

      it("moves to another table only after the connection is back", () => {
        store().navigateTab("t1", tableAt("wp_options"));

        expect(shown().table).toBe("wp_posts");
        expect(toasts()).toEqual([notice("open wp_options")]);
      });

      it("goes back to a row whose document is loaded and not to one that is not", async () => {
        connectedStore({ tabs: [tableTab({ nav: { back: [{ kind: "object", address: order4712 }], forward: [] } })] });
        lost("s1");

        await store().navBack();
        expect(selectActiveTab(store())?.kind).toBe("table");
        expect(toasts()).toEqual([notice("open wp_wc_orders #4712")]);

        loaded(order4712);
        await store().navBack();
        expect(selectActiveTab(store())?.kind).toBe("object");
      });

      it("goes forward to a table only after the connection is back", async () => {
        connectedStore({ tabs: [tableTab({ nav: { back: [], forward: [{ kind: "table", address: tableAt("wp_options") }] } })] });
        lost("s1");

        await store().navForward();

        expect(shown().table).toBe("wp_posts");
        expect(toasts()).toEqual([notice("open wp_options")]);
      });
    });

    describe("readouts", () => {
      const key = readoutKey("s1", "db");

      it("keeps a loaded readout instead of reading it again", async () => {
        connectedStore({ tabs: [] });
        useStore.setState({ readouts: { [key]: { version: 0, readout: readoutOf(), error: null } } });
        store().bumpData();
        lost("s1");

        await store().loadReadout("s1", "db");

        expect(mocked.readout).not.toHaveBeenCalled();
        expect(store().readouts[key].readout).not.toBeNull();
      });

      it("reads a readout that was never loaded", async () => {
        connectedStore({ tabs: [] });
        lost("s1");

        await store().loadReadout("s1", "db");

        expect(mocked.readout).toHaveBeenCalledWith("s1", "db");
      });
    });

    describe("live mode", () => {
      const liveKey = documentKey("s1", documentText(orderAddress));

      beforeEach(() => {
        connectedStore({ tabs: [] });
        store().openObject(orderAddress);
        useStore.setState({ live: { [liveKey]: { sessionId: "s1", address: documentText(orderAddress), id: "w1" } } });
      });

      it("polls a watched row while the connection is fine", async () => {
        await store().pollLive();

        expect(mocked.watchPoll).toHaveBeenCalledWith("s1", "w1");
      });

      it("does not poll a watched row of a lost session and keeps the watch", async () => {
        lost("s1");

        await store().pollLive();

        expect(mocked.watchPoll).not.toHaveBeenCalled();
        expect(store().live[liveKey]).toBeDefined();
        expect(toasts()).toEqual([]);
      });
    });
  });
});
