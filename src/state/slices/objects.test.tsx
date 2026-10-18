import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api, type Address, type Document, type Provenance } from "../../api";
import { installCommands } from "../../commands";
import { address, formatAddress } from "../../lib/address";
import { documentText } from "../../lib/document";
import { columnMeta, connectedStore, text, value } from "../../test/fixtures";
import { activeObject, objectDocument, orderAddress, paidViaSource, source } from "../../test/objectFixtures";
import { selectActiveTab, selectDocument, useStore } from "../store";
import type { SelectedCell } from "./tabs";

const mocked = vi.mocked(api, true);

const rowOf = (table: string, key: string): Address => address("shop", "db", { kind: "row", table, key });
const rows4711 = { ...orderAddress, view: "rows" as const };
const order4712 = rowOf("wp_wc_orders", "4712");

const store = () => useStore.getState();

function selection(tabId: string, selected: Provenance | null): SelectedCell {
  return {
    tabId,
    row: 0,
    col: 3,
    column: columnMeta("meta_value", "TEXT"),
    cell: text("bacs"),
    source: { sessionId: "s1", resultId: 1 },
    value: value({ raw: text("bacs"), source: selected }),
  };
}

function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

describe("object tabs", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [] });
    installCommands();
    mocked.document.mockResolvedValue(objectDocument());
  });

  describe("openObject", () => {
    it("opens a row address as the active object tab in the document view", () => {
      store().openObject(orderAddress);
      const tab = activeObject();
      expect(tab).toMatchObject({ sessionId: "s1", title: "wp_wc_orders #4711", view: "document", focus: null, preview: false });
      expect(formatAddress(tab.address)).toBe("databee://shop/db/wp_wc_orders/4711");
    });

    it("opens an address that names the Rows view in the rows view", () => {
      store().openObject(rows4711);
      expect(activeObject().view).toBe("rows");
      expect(formatAddress(activeObject().address)).toBe("databee://shop/db/wp_wc_orders/4711?view=rows");
    });

    it("reuses the open tab of the same row and shows the view the address names", () => {
      store().openObject(orderAddress);
      store().openObject(rows4711);
      expect(store().tabs).toHaveLength(1);
      expect(activeObject().view).toBe("rows");
    });

    it("keeps the shown view when the address names none", () => {
      store().openObject(rows4711);
      store().openObject(orderAddress);
      expect(store().tabs).toHaveLength(1);
      expect(activeObject().view).toBe("rows");
    });

    it("pins a preview tab only when the row is opened again with preview false", () => {
      store().openObject(orderAddress, { preview: true });
      store().openObject(orderAddress);
      expect(activeObject().preview).toBe(true);
      store().openObject(orderAddress, { preview: false });
      expect(activeObject().preview).toBe(false);
    });

    it("keeps a pinned tab pinned when the row is previewed again", () => {
      store().openObject(orderAddress);
      store().openObject(orderAddress, { preview: true });
      expect(activeObject().preview).toBe(false);
    });

    it("tells Rust to remember the row as a recent object", () => {
      store().openObject(orderAddress);
      expect(mocked.rememberObject).toHaveBeenCalledWith("s1", "databee://shop/db/wp_wc_orders/4711");
    });

    it("toasts when Rust cannot keep the recent object", async () => {
      mocked.rememberObject.mockRejectedValueOnce({ kind: "io", message: "disk full", code: null });
      store().openObject(orderAddress);
      await vi.waitFor(() => expect(store().toasts.at(-1)?.message).toContain("disk full"));
    });

    it("ignores an address that is not a row", () => {
      store().openObject(address("shop", "db", { kind: "table", table: "wp_posts", filter: [] }));
      expect(store().tabs).toEqual([]);
    });

    it("toasts when no open connection has the address's slug", () => {
      store().openObject({ ...orderAddress, connection: "ghost" });
      expect(store().tabs).toEqual([]);
      expect(store().toasts.at(-1)?.message).toBe("No open connection is named ghost");
    });
  });

  describe("preview tabs", () => {
    it("lets an object preview replace a table preview and keeps the table in its history", () => {
      store().openTableTab("db", "wp_posts", undefined, undefined, { preview: true });
      store().openObject(orderAddress, { preview: true });
      expect(store().tabs).toHaveLength(1);
      expect(activeObject().nav.back.map((e) => [e.kind, formatAddress(e.address)])).toEqual([["table", "databee://shop/db/wp_posts"]]);
    });

    it("lets a table preview replace an object preview and keeps the row in its history", () => {
      store().openObject(orderAddress, { preview: true });
      store().openTableTab("db", "wp_posts", undefined, undefined, { preview: true });
      const tab = selectActiveTab(store());
      expect(store().tabs).toHaveLength(1);
      expect(tab).toMatchObject({ kind: "table", table: "wp_posts" });
      expect(tab?.nav.back.map((e) => [e.kind, formatAddress(e.address)])).toEqual([["object", "databee://shop/db/wp_wc_orders/4711"]]);
    });

    it("lets an object preview replace an object preview in its place", () => {
      store().openObject(orderAddress);
      store().openObject(rowOf("wp_wc_orders", "1"), { preview: true });
      store().openObject(order4712, { preview: true });
      expect(store().tabs.map((t) => t.title)).toEqual(["wp_wc_orders #4711", "wp_wc_orders #4712"]);
    });

    it("does not replace a pinned tab", () => {
      store().openObject(orderAddress);
      store().openObject(order4712, { preview: true });
      expect(store().tabs).toHaveLength(2);
    });
  });

  describe("openAddress", () => {
    it("opens a row address as an object tab", async () => {
      await store().openAddress("databee://shop/db/wp_wc_orders/4711");
      expect(activeObject()).toMatchObject({ title: "wp_wc_orders #4711", view: "document", preview: false });
    });

    it("opens a row address with view=rows in the rows view, as a preview when asked", async () => {
      await store().openAddress("databee://shop/db/wp_wc_orders/4711?view=rows", { preview: true });
      expect(activeObject()).toMatchObject({ view: "rows", preview: true });
    });
  });

  describe("openEntity", () => {
    it("opens the object tab of the row the entity names without fetching its document again", async () => {
      await store().openEntity("order", "4711");
      expect(mocked.document).toHaveBeenCalledWith("s1", "order 4711", "db");
      const tab = activeObject();
      expect(formatAddress(tab.address)).toBe("databee://shop/db/wp_wc_orders/4711");
      await store().loadDocument("s1", documentText(tab.address), "db");
      expect(mocked.document).not.toHaveBeenCalledWith("s1", "databee://shop/db/wp_wc_orders/4711", "db");
    });

    it("toasts and opens nothing when Rust cannot build the document", async () => {
      mocked.document.mockRejectedValue({ kind: "invalid_input", message: "No order 9999", code: null });
      await store().openEntity("order", "9999");
      expect(store().tabs).toEqual([]);
      expect(store().toasts.at(-1)?.message).toContain("Could not open order #9999: No order 9999");
    });
  });

  describe("setObjectView", () => {
    it("shows the rows and records the document address for back", () => {
      store().openObject(orderAddress);
      store().setObjectView(activeObject().id, "rows");
      const tab = activeObject();
      expect(tab.view).toBe("rows");
      expect(formatAddress(tab.address)).toBe("databee://shop/db/wp_wc_orders/4711?view=rows");
      expect(tab.nav.back.map((e) => [e.kind, formatAddress(e.address)])).toEqual([["object", "databee://shop/db/wp_wc_orders/4711"]]);
    });

    it("records no history when the view is already shown", () => {
      store().openObject(orderAddress);
      store().setObjectView(activeObject().id, "document");
      expect(activeObject().nav.back).toEqual([]);
    });

    it("moves the focus along when one is given and keeps it otherwise", () => {
      store().openObject(orderAddress);
      const { id } = activeObject();
      store().focusObject(id, paidViaSource);
      store().setObjectView(id, "rows");
      expect(activeObject().focus).toEqual(paidViaSource);
      store().setObjectView(id, "document", source("wp_wc_orders", "status", "4711"));
      expect(activeObject().focus).toEqual(source("wp_wc_orders", "status", "4711"));
    });

    it("clears the selected cell of the grids being left, not another tab's", () => {
      store().openObject(orderAddress);
      const { id } = activeObject();
      useStore.setState({ selectedCell: selection(id, paidViaSource) });
      store().setObjectView(id, "rows");
      expect(store().selectedCell).toBeNull();

      useStore.setState({ selectedCell: selection("other-tab", paidViaSource) });
      store().setObjectView(id, "document");
      expect(store().selectedCell?.tabId).toBe("other-tab");
    });
  });

  describe("toggleObjectView", () => {
    it("goes from the document to the rows keeping the focused value", () => {
      store().openObject(orderAddress);
      const { id } = activeObject();
      store().focusObject(id, paidViaSource);
      store().toggleObjectView(id);
      expect(activeObject()).toMatchObject({ view: "rows", focus: paidViaSource });
    });

    it("goes from the rows to the document focusing the selected cell's value", () => {
      store().openObject(rows4711);
      const { id } = activeObject();
      useStore.setState({ selectedCell: selection(id, paidViaSource) });
      store().toggleObjectView(id);
      expect(activeObject()).toMatchObject({ view: "document", focus: paidViaSource });
    });

    it("keeps the focus when no cell is selected in the rows", () => {
      store().openObject(rows4711);
      const { id } = activeObject();
      store().focusObject(id, paidViaSource);
      store().toggleObjectView(id);
      expect(activeObject()).toMatchObject({ view: "document", focus: paidViaSource });
    });

    it("keeps the focus when the selected cell belongs to another tab or has no provenance", () => {
      store().openObject(rows4711);
      const { id } = activeObject();
      store().focusObject(id, paidViaSource);
      useStore.setState({ selectedCell: selection("other-tab", source("wp_wc_orders", "status", "4711")) });
      store().toggleObjectView(id);
      expect(activeObject().focus).toEqual(paidViaSource);

      store().setObjectView(id, "rows");
      useStore.setState({ selectedCell: selection(id, null) });
      store().toggleObjectView(id);
      expect(activeObject().focus).toEqual(paidViaSource);
    });
  });

  describe("history", () => {
    it("goes back to the document after the rows and forward again", async () => {
      store().openObject(orderAddress);
      store().setObjectView(activeObject().id, "rows");

      await store().navBack();
      expect(activeObject().view).toBe("document");
      expect(formatAddress(activeObject().address)).toBe("databee://shop/db/wp_wc_orders/4711");

      await store().navForward();
      expect(activeObject().view).toBe("rows");
    });

    it("keeps the focused value when back stays on the same row", async () => {
      store().openObject(orderAddress);
      const { id } = activeObject();
      store().focusObject(id, paidViaSource);
      store().setObjectView(id, "rows");
      await store().navBack();
      expect(activeObject().focus).toEqual(paidViaSource);
    });

    it("goes back to the previously previewed row and forward to the next", async () => {
      store().openObject(orderAddress, { preview: true });
      store().openObject(order4712, { preview: true });
      expect(activeObject().title).toBe("wp_wc_orders #4712");

      await store().navBack();
      expect(activeObject()).toMatchObject({ title: "wp_wc_orders #4711", focus: null });
      expect(formatAddress(activeObject().address)).toBe("databee://shop/db/wp_wc_orders/4711");

      await store().navForward();
      expect(activeObject().title).toBe("wp_wc_orders #4712");
    });

    it("drops the selected cell when stepping through history", async () => {
      store().openObject(orderAddress);
      const { id } = activeObject();
      store().setObjectView(id, "rows");
      useStore.setState({ selectedCell: selection(id, paidViaSource) });
      await store().navBack();
      expect(store().selectedCell).toBeNull();
    });
  });

  describe("loadDocument", () => {
    it("stores the document under the session and the requested text", async () => {
      await store().loadDocument("s1", "order 4711", "db");
      expect(mocked.document).toHaveBeenCalledWith("s1", "order 4711", "db");
      expect(selectDocument(store(), "s1", "order 4711")).toMatchObject({ document: { entity: "order" }, error: null });
    });

    it("stores the error and no document when Rust rejects the request", async () => {
      mocked.document.mockRejectedValue({ kind: "invalid_input", message: "No such row", code: null });
      await store().loadDocument("s1", "order 9999", "db");
      expect(selectDocument(store(), "s1", "order 9999")).toMatchObject({ document: null, error: { kind: "invalid_input", message: "No such row" } });
    });

    it("loads again after the data may have changed", async () => {
      await store().loadDocument("s1", "order 4711", "db");
      mocked.document.mockResolvedValue(objectDocument({ entity: "shop_order" }));
      store().bumpData();
      await store().loadDocument("s1", "order 4711", "db");
      expect(selectDocument(store(), "s1", "order 4711")?.document?.entity).toBe("shop_order");
    });

    it("keeps showing the previous document while the reload is pending", async () => {
      await store().loadDocument("s1", "order 4711", "db");
      const reply = deferred<Document>();
      mocked.document.mockReturnValue(reply.promise);
      store().bumpData();
      const loading = store().loadDocument("s1", "order 4711", "db");
      expect(selectDocument(store(), "s1", "order 4711")?.document?.entity).toBe("order");
      reply.resolve(objectDocument({ entity: "shop_order" }));
      await loading;
      expect(selectDocument(store(), "s1", "order 4711")?.document?.entity).toBe("shop_order");
    });

    it("drops a reply that a newer load has overtaken", async () => {
      const slow = deferred<Document>();
      mocked.document.mockReturnValueOnce(slow.promise);
      const first = store().loadDocument("s1", "order 4711", "db");
      store().bumpData();
      mocked.document.mockResolvedValue(objectDocument({ entity: "shop_order" }));
      await store().loadDocument("s1", "order 4711", "db");
      slow.resolve(objectDocument({ entity: "stale" }));
      await first;
      expect(selectDocument(store(), "s1", "order 4711")?.document?.entity).toBe("shop_order");
    });
  });

  describe("keepDocument", () => {
    it("serves a document fetched elsewhere without asking Rust again", async () => {
      const document = objectDocument({ entity: "kept" });
      store().keepDocument("s1", document);
      await store().loadDocument("s1", documentText(document.address), "db");
      expect(mocked.document).not.toHaveBeenCalled();
      expect(selectDocument(store(), "s1", documentText(document.address))?.document?.entity).toBe("kept");
    });
  });
});
