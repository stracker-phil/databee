import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import App from "../../App";
import { api, type Dashboard, type OrderSummary } from "../../api";
import { installCommands } from "../../commands";
import { selectWpRoute, useStore } from "../../state/store";
import { orderView, wpStore } from "../../test/wpFixtures";
import { CommandPalette } from "../CommandPalette";
import { PendingTray } from "../PendingTray";
import { CompareScreen, SnapshotsScreen } from "./Debugging";
import { WpWorkspace } from "./WpWorkspace";

const mocked = vi.mocked(api, true);
const now = Math.round(Date.now() / 1000);

const order = (id: string, status = "wc-processing"): OrderSummary => ({
  id,
  kind: "shop_order",
  status,
  total: "€49.90",
  gateway: "ppcp-gateway",
  gatewayTitle: "PayPal",
  email: "jane@shop.test",
  name: "Jane Doe",
  customerId: "12",
  created: now - 3600,
  itemCount: 2,
});

const dashboard: Dashboard = {
  orders: { total: 974, today: 12, byStatus: [{ value: "wc-completed", label: "Completed", count: 900 }], days: [{ date: "2026-10-15", completed: 3, processing: 1, failed: 1, other: 0 }], granularity: "day" },
  recentOrders: [order("3229")],
  autoloadBytes: 82_637,
  autoloadHealthy: true,
  transientsExpired: 11,
  actions: {
    total: 2611,
    byStatus: [{ value: "failed", label: "Failed", count: 3 }],
    recentFailed: [],
    byHook: [{ hook: "woocommerce_paypal_payments_check_pui_payment_captured", count: 38, lastFailure: now - 3 * 86400, lastMessage: "PayPal API returned 500", owner: "PayPal Payments" }],
    overdue: 0,
  },
  hposDrift: 0,
  tables: [
    { name: "wp_woocommerce_log", rows: 44_000, bytes: 1, category: "logs", owner: "WooCommerce" },
    { name: "wp_wc_orders_meta", rows: 9_500, bytes: 1, category: "woocommerce", owner: "WooCommerce" },
  ],
  insights: [
    { level: "warning", text: "wp_woocommerce_log holds 54% of all rows. Consider a retention rule.", action: { label: "Set retention", kind: "change", list: null, status: null, change: { type: "deleteLogsBefore", database: "db", days: 30, source: null } } },
    { level: "danger", text: "3 scheduled actions failed.", action: { label: "Review", kind: "navigate", list: "actions", status: "failed", change: null } },
  ],
};

describe("WordPress mode", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    wpStore();
    mocked.wpDashboard.mockResolvedValue(dashboard);
    mocked.wpBrowser.mockResolvedValue([{ list: "orders", label: "Orders", count: 974, breakdown: [] }]);
    mocked.wpOrders.mockResolvedValue({
      items: [order("3229"), order("3227", "wc-failed")],
      hasMore: false,
      facets: [
        { value: "wc-processing", label: "Processing", count: 1 },
        { value: "wc-failed", label: "Failed", count: 1 },
      ],
      kinds: [],
    });
    mocked.wpView.mockResolvedValue(orderView);
  });

  it("shows attention badges in the sidebar only for actionable statuses", async () => {
    mocked.wpBrowser.mockResolvedValue([
      { list: "orders", label: "Orders", count: 974, breakdown: [{ value: "wc-completed", label: "Completed", count: 900 }, { value: "wc-failed", label: "Failed", count: 63 }] },
      { list: "products", label: "Products", count: 32, breakdown: [{ value: "publish", label: "Published", count: 30 }, { value: "draft", label: "Draft", count: 2 }] },
      { list: "actions", label: "Scheduled actions", count: 2624, breakdown: [{ value: "failed", label: "Failed", count: 60 }] },
    ]);
    render(<WpWorkspace />);
    const nav = await screen.findByRole("navigation", { name: "WordPress" });
    expect(within(nav).getByText("63 failed")).toBeInTheDocument();
    expect(within(nav).getByText("60 failed")).toBeInTheDocument();
    expect(within(nav).getByText("974")).toBeInTheDocument();
    expect(within(nav).queryByText(/draft/i)).not.toBeInTheDocument();
    expect(within(nav).queryByText(/completed/i)).not.toBeInTheDocument();
  });

  it("opens WordPress databases on the dashboard", async () => {
    render(<App />);
    const ordersTile = (await screen.findByText("+12 today")).closest(".stat-tile")!;
    expect(within(ordersTile as HTMLElement).getByText("974")).toBeInTheDocument();
    expect(screen.getByText("80.7 KB")).toBeInTheDocument();
    expect(screen.getByText(/holds 54% of all rows/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Table rows" }));
    expect(screen.getByRole("button", { name: "woocommerce_log: 44k rows" })).toBeInTheDocument();
    expect(screen.getByText("HPOS in sync")).toBeInTheDocument();
  });

  it("turns alerts into actions: changes queue with SQL, reviews open filtered lists", async () => {
    render(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "Set retention" }));
    expect(mocked.planChange).toHaveBeenCalledWith("s1", { type: "deleteLogsBefore", database: "db", days: 30, source: null });
    expect(await screen.findByText("1 pending change")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Review" }));
    expect(selectWpRoute(useStore.getState())).toEqual({ screen: "list", list: "actions", params: { status: "failed" } });
  });

  it("groups failed actions by hook and expands them with their last log line", async () => {
    mocked.wpActions.mockResolvedValue({ items: [{ id: "77", hook: "woocommerce_paypal_payments_check_pui_payment_captured", status: "failed", scheduled: now - 3 * 86400, lastAttempt: now - 3 * 86400, attempts: 1, args: null, group: null, message: "PayPal API returned 500" }], hasMore: false, facets: [], kinds: [] });
    render(<App />);
    const group = await screen.findByRole("button", { name: /check_pui_payment_captured/ });
    expect(group).toHaveTextContent("×38");
    await userEvent.click(group);
    expect(await screen.findByText("PayPal API returned 500")).toBeInTheDocument();
    expect(mocked.wpActions).toHaveBeenCalledWith("s1", "db", expect.objectContaining({ status: "failed", kind: "woocommerce_paypal_payments_check_pui_payment_captured" }));
  });

  it("switches to the database view and back", async () => {
    render(<App />);
    await userEvent.click(screen.getByRole("button", { name: /Database/ }));
    expect(screen.getByRole("tree", { name: "Databases" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /WordPress/ }));
    expect(await screen.findByText("Recent orders")).toBeInTheDocument();
  });

  it("navigates from the dashboard to an order and back", async () => {
    render(<WpWorkspace />);
    await userEvent.click(await screen.findByRole("button", { name: /#3229/ }));
    expect(selectWpRoute(useStore.getState())).toEqual({ screen: "entity", entity: "order", id: "3229" });
    expect(await screen.findByText("City/postcode differs")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(selectWpRoute(useStore.getState())).toEqual({ screen: "dashboard" });
  });

  it("filters orders by status chips and search", async () => {
    useStore.getState().navigate({ screen: "list", list: "orders" });
    render(<WpWorkspace />);
    expect(await screen.findByText("#3227")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Failed 1/ }));
    expect(mocked.wpOrders).toHaveBeenLastCalledWith("s1", "db", "shop_order", expect.objectContaining({ status: "wc-failed" }));
    await userEvent.type(screen.getByLabelText("Search list"), "jane");
    await vi.waitFor(() => expect(mocked.wpOrders).toHaveBeenLastCalledWith("s1", "db", "shop_order", expect.objectContaining({ search: "jane" })));
  });

  it("sorts lists from the header row", async () => {
    useStore.getState().navigate({ screen: "list", list: "orders" });
    render(<WpWorkspace />);
    await screen.findByText("#3227");
    const total = screen.getByRole("columnheader", { name: "Total" });
    await userEvent.click(total);
    await vi.waitFor(() => expect(mocked.wpOrders).toHaveBeenLastCalledWith("s1", "db", "shop_order", expect.objectContaining({ sort: "total", descending: true })));
    expect(screen.getByRole("columnheader", { name: "Total" })).toHaveAttribute("aria-sort", "descending");
    await userEvent.click(screen.getByRole("columnheader", { name: "Total" }));
    await vi.waitFor(() => expect(mocked.wpOrders).toHaveBeenLastCalledWith("s1", "db", "shop_order", expect.objectContaining({ sort: "total", descending: false })));
  });

  it("opens rows in a side pane on wide windows, moves the selection with arrows and opens the page with Enter", async () => {
    window.innerWidth = 1600;
    useStore.getState().navigate({ screen: "list", list: "orders" });
    render(<WpWorkspace />);
    await userEvent.click(await screen.findByRole("row", { name: /#3229/ }));
    expect(selectWpRoute(useStore.getState())).toEqual({ screen: "list", list: "orders", selected: { entity: "order", id: "3229" } });
    expect(await screen.findByRole("complementary", { name: "Detail" })).toBeInTheDocument();
    expect(mocked.wpView).toHaveBeenCalledWith("s1", "db", "order", "3229");
    expect(screen.getByRole("row", { name: /#3229/ })).toHaveAttribute("aria-selected", "true");

    screen.getByRole("grid", { name: "orders list" }).focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(selectWpRoute(useStore.getState())).toEqual({ screen: "list", list: "orders", selected: { entity: "order", id: "3227" } });
    expect(useStore.getState().wpNav.stack.filter((r) => r.screen === "list")).toHaveLength(1);
    await userEvent.keyboard("{Enter}");
    expect(selectWpRoute(useStore.getState())).toEqual({ screen: "entity", entity: "order", id: "3227" });
  });

  it("falls back to full-page navigation on narrow windows", async () => {
    window.innerWidth = 1200;
    useStore.getState().navigate({ screen: "list", list: "orders" });
    render(<WpWorkspace />);
    await userEvent.click(await screen.findByRole("row", { name: /#3229/ }));
    expect(selectWpRoute(useStore.getState())).toEqual({ screen: "entity", entity: "order", id: "3229" });
    expect(screen.queryByRole("complementary", { name: "Detail" })).not.toBeInTheDocument();
  });

  it("shows an order with addresses, items, payment waterfall and timeline", async () => {
    mocked.wpOrderLogs.mockResolvedValue([
      {
        at: now - 1800,
        kind: "log",
        title: "PayPal API · capture order → 201",
        detail: "woocommerce-paypal-payments",
        level: "info",
        link: null,
        raw: "#7199.REST - POST https://api-m.sandbox.paypal.com/v2/checkout/orders/66M/capture\nResponse: Array ( [code] => 201 )",
        row: { table: "wp_woocommerce_log", idColumn: "log_id", id: "1732" },
      },
    ]);
    useStore.getState().openEntity("order", "4711");
    render(<WpWorkspace />);
    expect(await screen.findByText("Marktstraße 1")).toBeInTheDocument();
    expect(screen.getByText("MUG-1")).toBeInTheDocument();
    expect(screen.getByLabelText("Fees")).toHaveTextContent("gross €49.90→ fee €2.04→ net €47.86");
    const summary = await screen.findByRole("button", { name: /PayPal API · capture order → 201/ });
    expect(screen.queryByText(/#7199.REST/)).not.toBeInTheDocument();
    await userEvent.click(summary);
    expect(screen.getByText(/#7199.REST - POST/)).toBeInTheDocument();
    expect(screen.getByText("Order created")).toBeInTheDocument();
    await userEvent.click(within(screen.getByRole("list")).getByRole("button", { name: /Source row/ }));
    const tab = useStore.getState().tabs.at(-1)!;
    expect(tab.kind === "table" && [tab.table, tab.filter]).toEqual(["wp_woocommerce_log", "`log_id` = 1732"]);
    expect(useStore.getState().mode).toBe("db");
  });

  it("queues an order status change from the header", async () => {
    useStore.getState().openEntity("order", "4711");
    render(<WpWorkspace />);
    await userEvent.selectOptions(await screen.findByLabelText("Change status"), "completed");
    expect(mocked.planChange).toHaveBeenCalledWith("s1", { type: "setOrderStatus", database: "db", id: "4711", status: "completed" });
  });

  it("highlights fields that changed in live mode", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    useStore.getState().openEntity("order", "4711");
    render(<WpWorkspace />);
    await screen.findByText("Marktstraße 1");
    mocked.wpView.mockResolvedValue({ ...orderView, doc: { ...orderView.doc, fields: [orderView.doc.fields[0], { ...orderView.doc.fields[1], value: { t: "text", v: "wc-completed" } }] } });
    await userEvent.click(screen.getByRole("button", { name: /Live mode/ }));
    await vi.advanceTimersByTimeAsync(3100);
    await vi.waitFor(() => expect(screen.getByText(/1 changed/)).toBeInTheDocument());
    vi.useRealTimers();
  });
});

describe("Command palette", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    wpStore();
    installCommands();
  });

  it("finds pasted identifiers and opens the match", async () => {
    mocked.wpSearch.mockResolvedValue([{ entity: "order", id: "3229", title: "Order #3229", subtitle: null, matched: "_ppcp_paypal_order_id" }]);
    useStore.getState().setPaletteOpen(true);
    render(<CommandPalette />);
    await userEvent.type(screen.getByLabelText("Search everything"), "3UT60541JW9211534");
    expect(await screen.findByText("Order #3229")).toBeInTheDocument();
    expect(screen.getByText(/matched _ppcp_paypal_order_id/)).toBeInTheDocument();
    await userEvent.keyboard("{Enter}");
    expect(selectWpRoute(useStore.getState())).toEqual({ screen: "entity", entity: "order", id: "3229" });
    expect(useStore.getState().paletteOpen).toBe(false);
  });

  it("jumps to screens by name", async () => {
    useStore.getState().setPaletteOpen(true);
    render(<CommandPalette />);
    await userEvent.type(screen.getByLabelText("Search everything"), "failed act");
    await userEvent.click(screen.getByText("Failed actions"));
    expect(selectWpRoute(useStore.getState())).toEqual({ screen: "list", list: "actions", params: { status: "failed" } });
  });

  it("filters with prefixes and shows command bindings", async () => {
    useStore.getState().setPaletteOpen(true);
    render(<CommandPalette />);
    const input = screen.getByLabelText("Search everything");
    await userEvent.type(input, ">zoom");
    expect(screen.getByText("Zoom in")).toBeInTheDocument();
    expect(screen.getByText("⌘=")).toBeInTheDocument();
    expect(screen.queryByText("wp_options")).not.toBeInTheDocument();
    await userEvent.clear(input);
    await userEvent.type(input, "t:wp_opt");
    expect(screen.getByText("wp_options")).toBeInTheDocument();
    expect(screen.queryByText("Zoom in")).not.toBeInTheDocument();
    await userEvent.click(screen.getByText("wp_options"));
    expect(useStore.getState().mode).toBe("db");
    expect(useStore.getState().tabs.some((t) => t.kind === "table" && t.table === "wp_options")).toBe(true);
  });

  it("lists every entity type that has a numeric ID", async () => {
    mocked.wpSearch.mockResolvedValue([
      { entity: "order", id: "3229", title: "Order #3229", subtitle: null, matched: "id" },
      { entity: "post", id: "3229", title: "Post #3229", subtitle: "page", matched: "id" },
      { entity: "user", id: "3229", title: "User #3229", subtitle: null, matched: "id" },
    ]);
    useStore.getState().setPaletteOpen(true);
    render(<CommandPalette />);
    await userEvent.type(screen.getByLabelText("Search everything"), "#3229");
    expect(await screen.findByText("Order #3229")).toBeInTheDocument();
    expect(screen.getByText("Post #3229")).toBeInTheDocument();
    expect(screen.getByText("User #3229")).toBeInTheDocument();
    expect(mocked.wpSearch).toHaveBeenCalledWith("s1", "db", "3229");
  });
});

describe("Pending changes", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    wpStore();
  });

  it("applies queued SQL in one batch and can undo it", async () => {
    await useStore.getState().queueChange({ type: "retryAction", database: "db", id: "1" });
    await useStore.getState().queueChange({ type: "setAutoload", database: "db", option: "x", on: false });
    render(<PendingTray />);
    expect(screen.getByText("2 pending changes")).toBeInTheDocument();
    await userEvent.click(screen.getAllByRole("button", { name: "Show SQL" })[0]);
    expect(screen.getByText("-- retryAction;")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Apply/ }));
    expect(mocked.applyChanges).toHaveBeenCalledWith("s1", ["-- retryAction", "-- setAutoload"]);
    await userEvent.click(await screen.findByRole("button", { name: /Undo last apply/ }));
    expect(mocked.applyChanges).toHaveBeenLastCalledWith("s1", ["-- undo setAutoload", "-- undo retryAction"]);
  });

  it("discards single changes without writing", async () => {
    await useStore.getState().queueChange({ type: "retryAction", database: "db", id: "1" });
    render(<PendingTray />);
    await userEvent.click(screen.getByRole("button", { name: /Discard planned retryAction/ }));
    expect(screen.queryByText(/pending change/)).not.toBeInTheDocument();
    expect(mocked.applyChanges).not.toHaveBeenCalled();
  });
});

describe("Snapshots and compare", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    wpStore();
  });

  it("diffs a snapshot against now", async () => {
    mocked.wpSnapshots.mockResolvedValue([{ id: "a", label: "order #4711", scope: { entity: "order", id: "4711", includeRelated: true, optionPattern: null, includeActions: true }, created: new Date().toISOString(), paths: 40 }]);
    mocked.wpSnapshotDiff.mockResolvedValue([{ path: "order#4711/field/status", before: "wc-pending", after: "wc-processing" }]);
    render(<SnapshotsScreen />);
    await userEvent.click(await screen.findByRole("button", { name: /Diff now/ }));
    const row = (await screen.findByText("order#4711/field/status")).closest("tr")!;
    expect(within(row).getByText("wc-pending")).toBeInTheDocument();
    expect(within(row).getByText("wc-processing")).toBeInTheDocument();
  });

  it("compares two entities showing only differences by default", async () => {
    mocked.wpCompare.mockResolvedValue([
      { path: "field/status", a: "wc-processing", b: "wc-failed" },
      { path: "field/currency", a: "EUR", b: "EUR" },
    ]);
    render(<CompareScreen entity="order" a="4711" b="4713" />);
    expect(await screen.findByText("field/status")).toBeInTheDocument();
    expect(screen.queryByText("field/currency")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("checkbox"));
    expect(screen.getByText("field/currency")).toBeInTheDocument();
  });
});
