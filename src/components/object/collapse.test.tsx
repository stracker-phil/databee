import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import App from "../../App";
import { api, EMPTY_UI_STATE, type Address } from "../../api";
import { address } from "../../lib/address";
import { selectActiveTab, useStore } from "../../state/store";
import { connectedStore, table } from "../../test/fixtures";
import { browseOrderTables, objectDocument, orderAddress } from "../../test/objectFixtures";
import { session } from "../../test/session";

const mocked = vi.mocked(api, true);

const rows4711: Address = { ...orderAddress, view: "rows" };
const META = "wp_wc_orders_meta";
const tables = ["wp_wc_orders", META, "wp_wc_order_addresses", "wp_wc_order_items", "wp_wc_order_coupon_lookup"].map((name) => table(name));

const rowAddress = (name: string, key: string, database = "db") => address(session.slug, database, { kind: "row", table: name, key });

function seedCollapsed(keys: string[], opts: { identity?: string; database?: string } = {}) {
  const { identity = session.identity, database = "db" } = opts;
  useStore.setState({ uiState: { [identity]: { ...EMPTY_UI_STATE, collapsedGroups: { [database]: keys } } } });
}

async function openObject(target: Address = orderAddress) {
  render(<App />);
  await screen.findByRole("tree");
  act(() => useStore.getState().openObject(target));
  await screen.findByRole("region", { name: "Object header" });
}

const collapseMeta = () => screen.getByRole("button", { name: `Collapse ${META}` });
const expandMeta = () => screen.getByRole("button", { name: `Expand ${META}` });

describe("collapsible object sections", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [], tables });
    mocked.document.mockResolvedValue(objectDocument());
    mocked.browseTable.mockImplementation(browseOrderTables);
  });

  describe("Document view", () => {
    it("hides a group's rows on Collapse and keeps its band with aria-expanded false", async () => {
      await openObject();
      expect(collapseMeta()).toHaveAttribute("aria-expanded", "true");
      await userEvent.click(collapseMeta());
      expect(screen.queryByText("bacs")).not.toBeInTheDocument();
      expect(screen.queryByText("Happy birthday")).not.toBeInTheDocument();
      expect(expandMeta()).toHaveAttribute("aria-expanded", "false");
      expect(screen.getByText("2 keys · stored order")).toBeInTheDocument();
      expect(screen.getByText("Berlin")).toBeInTheDocument();
    });

    it("brings the rows back on Expand", async () => {
      await openObject();
      await userEvent.click(collapseMeta());
      await userEvent.click(expandMeta());
      expect(screen.getByText("bacs")).toBeInTheDocument();
      expect(collapseMeta()).toHaveAttribute("aria-expanded", "true");
    });

    it.each([["{Enter}"], [" "]])("toggles the focused chevron on %j", async (key) => {
      await openObject();
      collapseMeta().focus();
      await userEvent.keyboard(key);
      expect(screen.queryByText("bacs")).not.toBeInTheDocument();
      await userEvent.keyboard(key);
      expect(screen.getByText("bacs")).toBeInTheDocument();
    });

    it.each([["count", "2 keys · stored order"], ["relation", "meta · order_id"]])("toggles when the band's %s text is clicked", async (_part, label) => {
      await openObject();
      await userEvent.click(screen.getByText(label));
      expect(screen.queryByText("bacs")).not.toBeInTheDocument();
      await userEvent.click(screen.getByText(label));
      expect(screen.getByText("bacs")).toBeInTheDocument();
    });

    it("opens a table tab instead of collapsing when the table name is clicked", async () => {
      await openObject();
      await userEvent.click(screen.getByRole("button", { name: META }));
      await waitFor(() => expect(selectActiveTab(useStore.getState())).toMatchObject({ kind: "table", table: META }));
    });

    it("hides the +n more button of a collapsed group and leaves Also linked alone", async () => {
      await openObject();
      expect(screen.getByRole("button", { name: "+28 rows" })).toBeInTheDocument();
      await userEvent.click(screen.getByRole("button", { name: "Collapse wp_wc_order_items" }));
      expect(screen.queryByRole("button", { name: "+28 rows" })).not.toBeInTheDocument();
      expect(screen.getByText(/Also linked:/)).toBeInTheDocument();
      await userEvent.click(screen.getByRole("button", { name: "Expand wp_wc_order_items" }));
      expect(screen.getByRole("button", { name: "+28 rows" })).toBeInTheDocument();
    });
  });

  describe("Rows view", () => {
    it("hides a group's grid on Collapse, keeps its band, and shows it again on Expand", async () => {
      await openObject(rows4711);
      expect(await screen.findByText("bacs")).toBeInTheDocument();
      const section = screen.getByRole("region", { name: `${META} rows` });
      expect(within(section).getByText("WHERE order_id = 4711")).toBeInTheDocument();
      await userEvent.click(collapseMeta());
      expect(screen.queryByText("bacs")).not.toBeInTheDocument();
      expect(within(section).queryByText("WHERE order_id = 4711")).not.toBeInTheDocument();
      expect(section).toContainElement(expandMeta());
      expect(expandMeta()).toHaveAttribute("aria-expanded", "false");
      await userEvent.click(expandMeta());
      expect(await screen.findByText("bacs")).toBeInTheDocument();
    });

    it("does not browse a collapsed group's table until it is expanded", async () => {
      seedCollapsed([`wp_wc_orders::meta`]);
      await openObject(rows4711);
      await waitFor(() => expect(mocked.browseTable).toHaveBeenCalledWith("s1", expect.objectContaining({ table: "wp_wc_order_addresses" })));
      expect(mocked.browseTable).not.toHaveBeenCalledWith("s1", expect.objectContaining({ table: META }));
      await userEvent.click(expandMeta());
      await waitFor(() => expect(mocked.browseTable).toHaveBeenCalledWith("s1", expect.objectContaining({ table: META, where: [{ column: "order_id", value: "4711" }] })));
    });

    it("keeps the +n more scroll target working for groups that stay expanded", async () => {
      await openObject();
      await userEvent.click(screen.getByRole("button", { name: "+28 rows" }));
      expect(await screen.findByRole("region", { name: "wp_wc_order_items rows" })).toHaveAttribute("data-group", "items");
    });
  });

  describe("remembered per connection, database and document table", () => {
    it("shows the group collapsed when another row of the same table opens", async () => {
      await openObject();
      await userEvent.click(collapseMeta());
      act(() => useStore.getState().openObject(rowAddress("wp_wc_orders", "4712")));
      await waitFor(() => expect(mocked.document).toHaveBeenCalledWith("s1", "databee://shop/db/wp_wc_orders/4712", "db"));
      expect(await screen.findByRole("button", { name: `Expand ${META}` })).toHaveAttribute("aria-expanded", "false");
      expect(screen.queryByText("bacs")).not.toBeInTheDocument();
    });

    it("leaves the group expanded on a document of another table", async () => {
      seedCollapsed(["wp_wc_orders::meta"]);
      await openObject(rowAddress(META, "91"));
      expect(await screen.findByText("bacs")).toBeInTheDocument();
      expect(collapseMeta()).toHaveAttribute("aria-expanded", "true");
    });

    it("leaves the group expanded in another database", async () => {
      seedCollapsed(["wp_wc_orders::meta"]);
      await openObject(rowAddress("wp_wc_orders", "4711", "other"));
      expect(await screen.findByText("bacs")).toBeInTheDocument();
      expect(collapseMeta()).toHaveAttribute("aria-expanded", "true");
    });

    it("leaves the group expanded when only another connection collapsed it", async () => {
      seedCollapsed(["wp_wc_orders::meta"], { identity: "root@127.0.0.1:3307/other" });
      await openObject();
      expect(await screen.findByText("bacs")).toBeInTheDocument();
      expect(collapseMeta()).toHaveAttribute("aria-expanded", "true");
    });

    it("starts collapsed when the stored view state already lists the group", async () => {
      seedCollapsed(["wp_wc_orders::meta"]);
      await openObject();
      expect(expandMeta()).toHaveAttribute("aria-expanded", "false");
      expect(screen.queryByText("bacs")).not.toBeInTheDocument();
    });
  });

  describe("persistence", () => {
    it("sends the collapsed group to setUiState for the connection and database", async () => {
      await openObject();
      await userEvent.click(collapseMeta());
      await vi.waitFor(
        () =>
          expect(mocked.setUiState).toHaveBeenLastCalledWith(session.identity, expect.objectContaining({ collapsedGroups: { db: ["wp_wc_orders::meta"] } })),
        { timeout: 1500 },
      );
    });

    it("sends the group removed again after Expand", async () => {
      await openObject();
      await userEvent.click(collapseMeta());
      await userEvent.click(expandMeta());
      await vi.waitFor(() => expect(mocked.setUiState).toHaveBeenLastCalledWith(session.identity, expect.objectContaining({ collapsedGroups: { db: [] } })), { timeout: 1500 });
    });
  });
});
