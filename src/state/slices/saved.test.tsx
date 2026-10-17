import { act, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api, type SavedEntry } from "../../api";
import { Toasts } from "../../components/Toasts";
import { connectedStore, queryTab, session } from "../../test/fixtures";
import { selectActiveTab, useStore, type QueryTab } from "../store";

const mocked = vi.mocked(api, true);
const store = () => useStore.getState();
const queryTabs = () => store().tabs.filter((t): t is QueryTab => t.kind === "query");

const failed: SavedEntry = { id: "failed-orders", title: "Failed orders", sql: "SELECT * FROM wp_wc_orders", pin: null, origin: { kind: "project" }, readOnly: false };
const carts: SavedEntry = { id: "abandoned-carts", title: "Abandoned carts", sql: "SELECT * FROM wp_woocommerce_sessions", pin: null, origin: { kind: "preset", preset: "woocommerce" }, readOnly: true };
const savedRef = (entry: SavedEntry) => ({ id: entry.id, origin: entry.origin, database: "db", pin: entry.pin });

describe("saved queries", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [queryTab({ sql: "SELECT 1" })] });
  });

  describe("openSavedQuery", () => {
    it("activates the tab of an editable query that is already open without opening or running another", async () => {
      await act(async () => store().openSavedQuery("s1", "db", failed));
      await vi.waitFor(() => expect(mocked.execute).toHaveBeenCalledTimes(1));
      store().activateTab("q1");
      vi.clearAllMocks();

      await act(async () => store().openSavedQuery("s1", "db", failed));

      expect(queryTabs()).toHaveLength(2);
      expect(selectActiveTab(store())).toMatchObject({ title: "Failed orders", saved: { id: "failed-orders" } });
      expect(mocked.execute).not.toHaveBeenCalled();
    });

    it("opens a preset query as a new unsaved copy every time", async () => {
      await act(async () => store().openSavedQuery("s1", "db", carts));
      await act(async () => store().openSavedQuery("s1", "db", carts));
      expect(queryTabs().filter((t) => t.title === "copy of Abandoned carts")).toHaveLength(2);
    });
  });

  describe("saveQueryTab", () => {
    it("asks to write a query first when the SQL is blank", () => {
      connectedStore({ tabs: [queryTab({ sql: "  \n" })] });
      render(<Toasts />);
      act(() => store().saveQueryTab("q1"));
      expect(screen.getByText("Write a query first, then save it")).toBeInTheDocument();
      expect(store().dialog).toBeNull();
    });

    it("asks to choose a database first when the session has none", () => {
      connectedStore({ tabs: [queryTab({ sql: "SELECT 1" })] });
      useStore.setState({ sessions: [{ ...session, database: null }] });
      render(<Toasts />);
      act(() => store().saveQueryTab("q1"));
      expect(screen.getByText("Choose a database first: saved queries belong to one")).toBeInTheDocument();
      expect(store().dialog).toBeNull();
    });

    it("asks for a title when the query was never saved", () => {
      act(() => store().saveQueryTab("q1"));
      expect(store().dialog).toMatchObject({ type: "prompt", title: "Save query as" });
    });
  });

  describe("pinQueryTab", () => {
    it.each([
      ["was never saved", undefined],
      ["comes from a preset", savedRef(carts)],
    ])("asks to save first when the query %s", (_name, saved) => {
      connectedStore({ tabs: [queryTab({ sql: "SELECT 1", saved })] });
      render(<Toasts />);
      act(() => store().pinQueryTab("q1"));
      expect(screen.getByText("Save the query first, then pin it")).toBeInTheDocument();
      expect(store().dialog).toBeNull();
    });

    it("opens the pin dialog for a saved query", () => {
      connectedStore({ tabs: [queryTab({ sql: "SELECT 1", saved: savedRef(failed) })] });
      act(() => store().pinQueryTab("q1"));
      expect(store().dialog).toMatchObject({ type: "pin" });
    });
  });

  describe("copyPresetQuery", () => {
    it("reports the failure and keeps the preset origin", async () => {
      mocked.copyPresetQueryToProject.mockRejectedValue({ kind: "invalid_input", message: "No such query", code: null });
      connectedStore({ tabs: [queryTab({ sql: carts.sql, saved: savedRef(carts) })] });
      render(<Toasts />);
      await act(() => store().copyPresetQuery("q1"));
      expect(await screen.findByText("Could not copy the query to the project: No such query")).toBeInTheDocument();
      expect(queryTabs()[0].saved?.origin).toEqual({ kind: "preset", preset: "woocommerce" });
    });
  });

  describe("loadSavedQueries", () => {
    it("reports the failure and resolves to an empty list", async () => {
      mocked.savedQueries.mockRejectedValue({ kind: "invalid_input", message: "No project", code: null });
      render(<Toasts />);
      let entries: SavedEntry[] | undefined;
      await act(async () => {
        entries = await store().loadSavedQueries("s1", "db");
      });
      expect(entries).toEqual([]);
      expect(screen.getByText("Could not load saved queries: No project")).toBeInTheDocument();
    });
  });
});
