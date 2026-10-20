import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import App from "../../App";
import { api, type Address, type Value } from "../../api";
import { installCommands } from "../../commands";
import { formatAddress } from "../../lib/address";
import { selectActiveTab, useStore } from "../../state/store";
import { connectedStore, table, text, value } from "../../test/fixtures";
import { activeObject, browseOrderTables, objectDocument, orderAddress, orderGroups, paidViaSource, setStatus } from "../../test/objectFixtures";

const mocked = vi.mocked(api, true);

const rows4711: Address = { ...orderAddress, view: "rows" };
const ORDER_URL = "databee://shop/db/wp_wc_orders/4711";

const toDocument = "{Meta>}1{/Meta}";
const toRows = "{Meta>}2{/Meta}";

const tables = ["wp_wc_orders", "wp_wc_orders_meta", "wp_wc_order_addresses", "wp_wc_order_items", "wp_wc_order_coupon_lookup"].map((name) => table(name));

async function openOrder(address: Address = orderAddress) {
  render(<App />);
  await screen.findByRole("tree");
  act(() => useStore.getState().openObject(address));
  await screen.findByRole("region", { name: "Object header" });
}

const sidebarRow = (name: string) => within(screen.getByRole("tree")).getByText(name).closest<HTMLElement>('[role="treeitem"]')!;
const bandsOf = (name: string) => screen.getAllByRole("button", { name }).map((button) => button.closest<HTMLElement>("[data-table]")!);

describe("ObjectPanel", () => {
  const scrolled: Element[] = [];

  beforeEach(() => {
    vi.clearAllMocks();
    scrolled.length = 0;
    Element.prototype.scrollIntoView = function (this: Element) {
      scrolled.push(this);
    };
    connectedStore({ tabs: [], tables });
    installCommands();
    mocked.document.mockResolvedValue(objectDocument());
    mocked.browseTable.mockImplementation(browseOrderTables);
  });

  afterEach(() => {
    delete (Element.prototype as Partial<Element>).scrollIntoView;
  });

  describe("Document view", () => {
    it("shows a band per group with its relation and count, then the stored values", async () => {
      await openOrder();
      expect(screen.getByText("this row")).toBeInTheDocument();
      expect(screen.getByText("3 columns")).toBeInTheDocument();
      expect(screen.getByText("meta · order_id")).toBeInTheDocument();
      expect(screen.getByText("2 keys · stored order")).toBeInTheDocument();
      expect(screen.getByText("Addresses · order_id → this row")).toBeInTheDocument();
      expect(screen.getByText("Refunds · parent_order_id → this row")).toBeInTheDocument();
      expect(screen.getByText("Line items · order_id → this row")).toBeInTheDocument();
      expect(screen.getByText("30 rows")).toBeInTheDocument();
      expect(screen.getByText("every stored timestamp, in order")).toBeInTheDocument();
      for (const stored of ["wc-processing", "119.00", "_paid_via", "bacs", "Berlin", "Blue mug", "wc-refunded", "2025-01-02 10:00:00"]) {
        expect(screen.getByText(stored)).toBeInTheDocument();
      }
    });

    it("marks the object's table as selected in the sidebar", async () => {
      await openOrder();
      expect(sidebarRow("wp_wc_orders")).toHaveAttribute("aria-selected", "true");
      expect(sidebarRow("wp_wc_orders_meta")).toHaveAttribute("aria-selected", "false");
    });

    it("shows an error panel when Rust cannot build the document", async () => {
      mocked.document.mockRejectedValue({ kind: "invalid_input", message: "No such row", code: null });
      render(<App />);
      await screen.findByRole("tree");
      act(() => useStore.getState().openObject(orderAddress));
      expect(await screen.findByText("No such row")).toBeInTheDocument();
    });

    it("keeps the band of a failed group and shows its error instead of collapsing it", async () => {
      const failed = { ...orderGroups.refunds, total: 0, rows: [], error: "Query timed out" };
      mocked.document.mockResolvedValue(objectDocument({ groups: [orderGroups.own, failed] }));
      await openOrder();
      expect(await screen.findByRole("alert")).toHaveTextContent("Query timed out");
      expect(screen.getByText("Refunds · parent_order_id → this row")).toBeInTheDocument();
      expect(screen.queryByText(/Also linked:/)).not.toBeInTheDocument();
    });

    it("lists a linked table with no rows in the Also linked line instead of a band", async () => {
      await openOrder();
      expect(screen.getByText(/Also linked:/)).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "wp_wc_order_coupon_lookup" })).toHaveAttribute("title", "Coupons · order_id → this row · 0 rows");
      expect(screen.queryByText("Coupons · order_id → this row")).not.toBeInTheDocument();
    });

    it("opens the rows of an empty linked table in a preview table tab", async () => {
      await openOrder();
      await userEvent.click(screen.getByRole("button", { name: "wp_wc_order_coupon_lookup" }));
      await waitFor(() =>
        expect(selectActiveTab(useStore.getState())).toMatchObject({ kind: "table", table: "wp_wc_order_coupon_lookup", where: [{ column: "order_id", value: "4711" }], preview: true }),
      );
    });

    it("opens a group's rows in a table tab from its band", async () => {
      await openOrder();
      await userEvent.click(screen.getByRole("button", { name: "wp_wc_order_addresses" }));
      await waitFor(() => expect(selectActiveTab(useStore.getState())).toMatchObject({ kind: "table", table: "wp_wc_order_addresses", where: [{ column: "order_id", value: "4711" }] }));
    });

    it("opens the row another primary key addresses as a new column of the same tab", async () => {
      await openOrder();
      await userEvent.click(screen.getByRole("link", { name: "4712" }));
      const tab = activeObject();
      expect(formatAddress(tab.address)).toBe("databee://shop/db/wp_wc_orders/4712");
      expect(tab.columns.map((c) => c.kind === "object" && formatAddress(c.address))).toEqual([ORDER_URL, "databee://shop/db/wp_wc_orders/4712"]);
      expect(tab.active).toBe(1);
      expect(useStore.getState().tabs).toHaveLength(1);
    });

    it("switches to the Rows view when the object's own primary key is clicked", async () => {
      await openOrder();
      await userEvent.click(screen.getByRole("link", { name: "4711" }));
      expect(activeObject().view).toBe("rows");
      expect(useStore.getState().tabs).toHaveLength(1);
    });
  });

  describe("view control", () => {
    it("switches to the Rows view and back", async () => {
      await openOrder();
      expect(screen.getByRole("button", { name: "Document" })).toHaveAttribute("aria-pressed", "true");
      await userEvent.click(screen.getByRole("button", { name: "Rows" }));
      expect(screen.getByRole("button", { name: "Rows" })).toHaveAttribute("aria-pressed", "true");
      expect(formatAddress(activeObject().address)).toBe(`${ORDER_URL}?view=rows`);
      await userEvent.click(screen.getByRole("button", { name: "Document" }));
      expect(formatAddress(activeObject().address)).toBe(ORDER_URL);
    });

    it("shows a grid of each group's table in the Rows view and leaves the empty group out", async () => {
      await openOrder(rows4711);
      expect(await screen.findByText("bacs")).toBeInTheDocument();
      const meta = screen.getByRole("region", { name: "wp_wc_orders_meta rows" });
      expect(within(meta).getByText("WHERE order_id = 4711")).toBeInTheDocument();
      expect(screen.queryByRole("region", { name: "wp_wc_order_coupon_lookup rows" })).not.toBeInTheDocument();
      expect(mocked.browseTable).toHaveBeenCalledWith("s1", expect.objectContaining({ table: "wp_wc_orders_meta", where: [{ column: "order_id", value: "4711" }] }));
    });
  });

  describe("capped groups", () => {
    it("offers +n rows only on the capped group", async () => {
      await openOrder();
      expect(screen.getAllByRole("button", { name: /^\+\d+ (rows|keys)$/ })).toHaveLength(1);
      expect(screen.getByRole("button", { name: "+28 rows" })).toBeInTheDocument();
    });

    it("switches to Rows and scrolls to the group's section", async () => {
      await openOrder();
      await userEvent.click(screen.getByRole("button", { name: "+28 rows" }));
      expect(activeObject().view).toBe("rows");
      const section = await screen.findByRole("region", { name: "wp_wc_order_items rows" });
      expect(section).toHaveAttribute("data-group", "items");
      await waitFor(() => expect(scrolled).toContain(section));
    });
  });

  describe("⌘1 / ⌘2 (object.viewDocument / object.viewRows)", () => {
    it("selects the focused value's cell in the Rows view", async () => {
      await openOrder();
      await userEvent.click(screen.getByText("bacs"));
      await userEvent.keyboard(toRows);
      await waitFor(() => expect(useStore.getState().selectedCell?.value?.source).toEqual(paidViaSource));
      expect(activeObject().view).toBe("rows");
      expect(useStore.getState().selectedCell?.tabId).toBe(activeObject().id);
    });

    it("focuses the selected cell's value in the Document view", async () => {
      await openOrder(rows4711);
      await userEvent.click(await screen.findByText("bacs"));
      expect(useStore.getState().selectedCell?.value?.source).toEqual(paidViaSource);
      await userEvent.keyboard(toDocument);
      expect(activeObject()).toMatchObject({ view: "document", focus: paidViaSource });
      expect(useStore.getState().selectedCell).toBeNull();
      expect(await screen.findByRole("button", { name: "Document" })).toHaveAttribute("aria-pressed", "true");
    });

    it("keeps the same value across a round trip", async () => {
      await openOrder();
      await userEvent.click(screen.getByText("Happy birthday"));
      await userEvent.keyboard(toRows);
      await waitFor(() => expect(useStore.getState().selectedCell?.value?.raw).toEqual({ t: "text", v: "Happy birthday" }));
      await userEvent.keyboard(toDocument);
      expect(activeObject()).toMatchObject({ view: "document", focus: { table: "wp_wc_orders_meta", column: "meta_value", row: [{ column: "id", value: "92" }] } });
    });

    it("keeps the Document view when Tab is pressed in the value editor", async () => {
      await openOrder();
      await userEvent.click(screen.getByText("bacs"));
      await userEvent.keyboard("e");
      await screen.findByLabelText("Edit value");
      await userEvent.keyboard("{Tab}");
      expect(activeObject().view).toBe("document");
    });

    it("keeps the Document view when Tab is pressed", async () => {
      await openOrder();
      await userEvent.keyboard("{Tab}");
      expect(activeObject().view).toBe("document");
      expect(screen.getByRole("button", { name: "Document" })).toHaveAttribute("aria-pressed", "true");
    });

    it("keeps the Rows view when Tab is pressed", async () => {
      await openOrder(rows4711);
      await userEvent.keyboard("{Tab}");
      expect(activeObject().view).toBe("rows");
      expect(screen.getByRole("button", { name: "Rows" })).toHaveAttribute("aria-pressed", "true");
    });

    it("does nothing for ⌘1 while already in the Document view", async () => {
      await openOrder();
      const before = formatAddress(activeObject().address);
      await userEvent.keyboard(toDocument);
      expect(activeObject().view).toBe("document");
      expect(formatAddress(activeObject().address)).toBe(before);
    });

    it("does nothing for ⌘2 while already in the Rows view", async () => {
      await openOrder(rows4711);
      const before = formatAddress(activeObject().address);
      await userEvent.keyboard(toRows);
      expect(activeObject().view).toBe("rows");
      expect(formatAddress(activeObject().address)).toBe(before);
    });
  });

  describe("per-view scroll", () => {
    const container = () => {
      const el = document.querySelector<HTMLElement>('[data-column="0"] .object')!;
      Object.defineProperty(el, "scrollHeight", { configurable: true, value: 10000 });
      Object.defineProperty(el, "clientHeight", { configurable: true, value: 100 });
      return el;
    };

    function scrollTo(el: HTMLElement, top: number) {
      el.scrollTop = top;
      fireEvent.scroll(el);
    }

    it("keeps a scroll position per view and starts a first-visited view at the top", async () => {
      await openOrder();
      scrollTo(container(), 400);
      await userEvent.keyboard(toRows);
      await waitFor(() => expect(activeObject().view).toBe("rows"));
      expect(container().scrollTop).toBe(0);

      scrollTo(container(), 150);
      await userEvent.keyboard(toDocument);
      await waitFor(() => expect(activeObject().view).toBe("document"));
      expect(container().scrollTop).toBe(400);

      await userEvent.keyboard(toRows);
      await waitFor(() => expect(activeObject().view).toBe("rows"));
      expect(container().scrollTop).toBe(150);
    });

    it("leaves focus where it was when ⌘2 switches to the Rows view", async () => {
      await openOrder();
      const before = document.activeElement;
      await userEvent.keyboard(toRows);
      await waitFor(() => expect(activeObject().view).toBe("rows"));
      expect(document.activeElement).toBe(before);
    });
  });

  describe("editing a meta value", () => {
    it("queues a setCell on the meta row's id after e, typing and Enter", async () => {
      await openOrder();
      await userEvent.click(screen.getByText("bacs"));
      await userEvent.keyboard("e");
      const input = await screen.findByLabelText("Edit value");
      expect(input).toHaveValue("bacs");
      await userEvent.clear(input);
      await userEvent.type(input, "paypal{Enter}");
      await waitFor(() =>
        expect(mocked.planChange).toHaveBeenCalledWith("s1", { type: "setCell", database: "db", table: "wp_wc_orders_meta", keyColumn: "id", key: "91", column: "meta_value", value: "paypal" }),
      );
      expect(screen.queryByLabelText("Edit value")).not.toBeInTheDocument();
    });

    it("queues an edit of the own row by its primary key", async () => {
      await openOrder();
      await userEvent.click(screen.getByText("wc-processing"));
      await userEvent.keyboard("e");
      const input = await screen.findByLabelText("Edit value");
      await userEvent.clear(input);
      await userEvent.type(input, "wc-completed{Enter}");
      await waitFor(() =>
        expect(mocked.planChange).toHaveBeenCalledWith("s1", { type: "setCell", database: "db", table: "wp_wc_orders", keyColumn: "id", key: "4711", column: "status", value: "wc-completed" }),
      );
    });

    it("queues nothing when Escape cancels the edit", async () => {
      await openOrder();
      await userEvent.click(screen.getByText("bacs"));
      await userEvent.keyboard("e");
      await userEvent.type(await screen.findByLabelText("Edit value"), "x{Escape}");
      expect(screen.queryByLabelText("Edit value")).not.toBeInTheDocument();
      expect(mocked.planChange).not.toHaveBeenCalled();
    });

    it("does nothing on e without a focused value", async () => {
      await openOrder();
      await userEvent.keyboard("e");
      expect(screen.queryByLabelText("Edit value")).not.toBeInTheDocument();
    });
  });

  describe("actions", () => {
    it("lists the document's actions in the Actions menu and none that it does not list", async () => {
      await openOrder();
      await userEvent.click(screen.getByRole("button", { name: "Actions" }));
      expect(screen.getByRole("menuitem", { name: /^Run mark-completed/ })).toBeInTheDocument();
      expect(screen.getByRole("menuitem", { name: /^Run purge-notes \(no undo\)/ })).toBeInTheDocument();
      expect(screen.queryByRole("menuitem", { name: /cancel-order/ })).not.toBeInTheDocument();
    });

    it("shows under each action what it writes", async () => {
      await openOrder();
      await userEvent.click(screen.getByRole("button", { name: "Actions" }));
      expect(screen.getByRole("menuitem", { name: /Run mark-completed/ })).toHaveTextContent("wp_wc_orders: date_updated_gmt, status · wp_posts: post_modified, post_status");
      expect(screen.getByRole("menuitem", { name: /Run purge-notes/ })).toHaveTextContent("wp_comments: free SQL");
    });

    it("opens the same menu on a right-click of the header", async () => {
      await openOrder();
      fireEvent.contextMenu(screen.getByRole("region", { name: "Object header" }));
      expect(screen.getByRole("menuitem", { name: /^Run mark-completed/ })).toBeInTheDocument();
    });

    it("previews the chosen action on the document address and queues that preview", async () => {
      await openOrder();
      await userEvent.click(screen.getByRole("button", { name: "Actions" }));
      await userEvent.click(screen.getByRole("menuitem", { name: /^Run mark-completed/ }));
      await waitFor(() => expect(mocked.previewAction).toHaveBeenCalledWith("s1", ORDER_URL, "mark-completed", {}));
      expect(mocked.queueActionPreview).not.toHaveBeenCalled();
      await userEvent.click(await screen.findByRole("button", { name: "Queue change" }));
      await waitFor(() => expect(mocked.queueActionPreview).toHaveBeenCalledWith("s1", 1));
      await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
      expect(useStore.getState().pending).toHaveLength(1);
    });

    it("shows each row the action writes with its old and new values, the row count and undo", async () => {
      const cell = (v: string | null): Value => value({ raw: v === null ? { t: "null" } : text(v), kind: v === null ? { kind: "null" } : { kind: "text" } });
      mocked.previewAction.mockResolvedValue({
        id: 7,
        description: "mark-completed: Update 1 wp_wc_orders row, 1 wp_posts row",
        canUndo: true,
        sql: [],
        rows: [
          { table: "wp_wc_orders", key: [{ column: "id", value: "4711" }], change: { kind: "update", columns: [{ column: "status", old: cell("wc-processing"), new: cell("wc-completed") }] } },
          { table: "wp_posts", key: [{ column: "ID", value: "4711" }], change: { kind: "update", columns: [{ column: "post_modified", old: cell(null), new: cell("2026-10-17 12:34:56") }] } },
        ],
      });
      await openOrder();
      await userEvent.click(screen.getByRole("button", { name: "Actions" }));
      await userEvent.click(screen.getByRole("menuitem", { name: /^Run mark-completed/ }));
      const order = await screen.findByRole("group", { name: "wp_wc_orders #4711" });
      expect(order).toHaveTextContent(/status\s*wc-processing\s*→\s*wc-completed/);
      expect(screen.getByRole("group", { name: "wp_posts #4711" })).toHaveTextContent(/post_modified\s*NULL\s*→\s*2026-10-17 12:34:56/);
      expect(screen.getByText(/2 rows · undo available/)).toBeInTheDocument();
      expect(screen.getByText(/review its SQL in the pending tray/)).toBeInTheDocument();
      await userEvent.click(screen.getByRole("button", { name: "Queue change" }));
      await waitFor(() => expect(mocked.queueActionPreview).toHaveBeenCalledWith("s1", 7));
    });

    it("shows the statements of a free SQL action, whose rows are not known in advance", async () => {
      mocked.previewAction.mockResolvedValue({ id: 2, description: "purge-notes: free SQL", canUndo: false, rows: [], sql: ["DELETE FROM wp_comments WHERE comment_type = 'order_note'"] });
      await openOrder();
      await userEvent.click(screen.getByRole("button", { name: "Actions" }));
      await userEvent.click(screen.getByRole("menuitem", { name: /^Run purge-notes/ }));
      expect(await screen.findByText("DELETE FROM wp_comments WHERE comment_type = 'order_note'")).toBeInTheDocument();
      expect(screen.getByText(/affects rows not known in advance/)).toBeInTheDocument();
      expect(screen.getByText(/rows not known in advance · no undo/)).toBeInTheDocument();
    });

    it("cancels without queueing anything", async () => {
      await openOrder();
      await userEvent.click(screen.getByRole("button", { name: "Actions" }));
      await userEvent.click(screen.getByRole("menuitem", { name: /^Run mark-completed/ }));
      await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      expect(mocked.queueActionPreview).not.toHaveBeenCalled();
    });

    it("previews on the document address while the Rows view is shown", async () => {
      await openOrder(rows4711);
      await userEvent.click(screen.getByRole("button", { name: "Actions" }));
      await userEvent.click(screen.getByRole("menuitem", { name: /^Run mark-completed/ }));
      await waitFor(() => expect(mocked.previewAction).toHaveBeenCalledWith("s1", ORDER_URL, "mark-completed", {}));
    });

    it("edits a parameter in the sheet, starting at the first choice, and previews again", async () => {
      mocked.document.mockResolvedValue(objectDocument({ actions: [setStatus] }));
      await openOrder();
      await userEvent.click(screen.getByRole("button", { name: "Actions" }));
      await userEvent.click(screen.getByRole("menuitem", { name: /^Run set-status/ }));
      const status = await screen.findByRole("combobox", { name: "status" });
      expect(status).toHaveValue("wc-completed");
      expect(screen.getByText("writes wp_wc_orders: status")).toBeInTheDocument();
      await waitFor(() => expect(mocked.previewAction).toHaveBeenCalledWith("s1", ORDER_URL, "set-status", { status: "wc-completed" }));
      await userEvent.clear(status);
      await userEvent.type(status, "wc-on-hold");
      await waitFor(() => expect(mocked.previewAction).toHaveBeenLastCalledWith("s1", ORDER_URL, "set-status", { status: "wc-on-hold" }));
      expect(mocked.queueActionPreview).not.toHaveBeenCalled();
    });

    it("hides the Actions button and says no action applies on a right-click when the document lists none", async () => {
      mocked.document.mockResolvedValue(objectDocument({ actions: [] }));
      await openOrder();
      expect(screen.queryByRole("button", { name: "Actions" })).not.toBeInTheDocument();
      fireEvent.contextMenu(screen.getByRole("region", { name: "Object header" }));
      expect(screen.getByRole("menuitem", { name: "No actions apply to this row" })).toBeDisabled();
    });

    it("lists the actions of the active object in the command palette, and only while it is active", async () => {
      await openOrder();
      await userEvent.keyboard("{Meta>}k{/Meta}");
      await userEvent.type(await screen.findByLabelText("Search everything"), "mark-comp");
      expect(await screen.findByRole("option", { name: /Run mark-completed/ })).toBeInTheDocument();
      await userEvent.click(screen.getByRole("option", { name: /Run mark-completed/ }));
      await waitFor(() => expect(mocked.previewAction).toHaveBeenCalledWith("s1", ORDER_URL, "mark-completed", {}));
    });

    it("leaves the actions out of the palette while another kind of tab is active", async () => {
      await openOrder();
      act(() => useStore.getState().openTableTab("db", "wp_wc_orders_meta"));
      await userEvent.keyboard("{Meta>}k{/Meta}");
      await userEvent.type(await screen.findByLabelText("Search everything"), "mark-comp");
      expect(screen.queryByRole("option", { name: /Run mark-completed/ })).not.toBeInTheDocument();
    });
  });

  describe("linked table highlight", () => {
    it("focuses the table of the bands, dimming the rest, when its sidebar row is hovered", async () => {
      await openOrder();
      expect(bandsOf("wp_wc_orders")).toHaveLength(2);
      await userEvent.hover(within(screen.getByRole("tree")).getByText("wp_wc_orders"));
      const focus = await screen.findByTestId("table-focus");
      expect(focus.textContent).toContain('.sh[data-table="wp_wc_orders"]');
      expect(focus.textContent).toContain("var(--dimmed)");
    });

    it("does not give a band a lit class of its own", async () => {
      await openOrder();
      await userEvent.hover(within(screen.getByRole("tree")).getByText("wp_wc_orders"));
      await screen.findByTestId("table-focus");
      for (const band of bandsOf("wp_wc_orders")) expect(band).not.toHaveClass("lit");
    });

    it("lights the sidebar row when a band is hovered", async () => {
      await openOrder();
      await userEvent.hover(screen.getByRole("button", { name: "wp_wc_orders_meta" }));
      await waitFor(() => expect(sidebarRow("wp_wc_orders_meta")).toHaveClass("lit"));
      expect(sidebarRow("wp_wc_orders")).not.toHaveClass("lit");
    });

    it("lights only the sidebar row, not the bands, when a value is hovered", async () => {
      await openOrder();
      await userEvent.hover(screen.getByText("119.00"));
      await waitFor(() => expect(sidebarRow("wp_wc_orders")).toHaveClass("lit"));
      expect(screen.queryByTestId("table-focus")).not.toBeInTheDocument();
    });
  });

  describe("openEntity", () => {
    it("opens an object tab titled by the entity, addressed by the row, from the fetched document", async () => {
      render(<App />);
      await screen.findByRole("tree");
      await act(async () => useStore.getState().openEntity("order", "4711"));
      const tab = await screen.findByRole("tab", { name: /^order #4711/ });
      expect(tab).toHaveAttribute("aria-selected", "true");
      expect(tab).toHaveAttribute("title", ORDER_URL);
      expect(await screen.findByRole("region", { name: "Object header" })).toBeInTheDocument();
      expect(mocked.document).toHaveBeenCalledWith("s1", "order 4711", "db");
      expect(mocked.document).not.toHaveBeenCalledWith("s1", ORDER_URL, "db");
    });
  });
});
