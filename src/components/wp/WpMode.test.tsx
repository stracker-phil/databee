import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import App from "../../App";
import { api, type Dashboard, type OrderSummary } from "../../api";
import { installCommands } from "../../commands";
import { selectWpRoute, useStore } from "../../state/store";
import { session } from "../../test/fixtures";
import { NOW, doc, orderView, wpInfo, wpStore } from "../../test/wpFixtures";
import { CommandPalette } from "../CommandPalette";
import { Dialogs } from "../Dialogs";
import { Toasts } from "../Toasts";
import { PendingTray } from "../PendingTray";
import { CompareScreen, SnapshotsScreen } from "./Debugging";
import { WpWorkspace } from "./WpWorkspace";

const mocked = vi.mocked(api, true);
const now = NOW / 1000;
const originalWidth = window.innerWidth;

afterEach(() => {
  window.innerWidth = originalWidth;
  vi.useRealTimers();
});

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
    byHook: [{ hook: "woocommerce_paypal_payments_check_pui_payment_captured", count: 38, lastFailure: now - 3 * 86400, lastMessage: "PayPal API returned 500" }],
    overdue: 0,
  },
  hposDrift: 0,
  tables: [
    { name: "wp_woocommerce_log", rows: 44_000, bytes: 1, category: "logs" },
    { name: "wp_wc_orders_meta", rows: 9_500, bytes: 1, category: "woocommerce" },
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
      limit: 50,
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

  it("groups the sidebar like wp-admin, whatever order the backend sends", async () => {
    mocked.wpBrowser.mockResolvedValue([
      { list: "orders", label: "Orders", count: 3, breakdown: [] },
      { list: "settings", label: "Options", count: 700, breakdown: [] },
      { list: "actions", label: "Scheduled actions", count: 5, breakdown: [] },
      { list: "users", label: "Users", count: 8, breakdown: [] },
      { list: "content", label: "Content", count: 41, breakdown: [] },
    ]);
    render(<WpWorkspace />);
    const group = async (name: string) => within(await screen.findByRole("group", { name })).getAllByRole("button").map((b) => b.textContent?.replace(/\d+/g, "").trim());
    expect(await group("WordPress")).toEqual(["Content", "Users", "Options"]);
    expect(await group("WooCommerce")).toEqual(["Orders"]);
    expect(await group("Tools")).toEqual(["Scheduled actions", "Snapshots & diff"]);
  });

  it("opens WordPress databases on the dashboard", async () => {
    render(<App />);
    const ordersTile = within(await screen.findByRole("group", { name: "Orders" }));
    expect(ordersTile.getByText("974")).toBeInTheDocument();
    expect(ordersTile.getByText("+12 today")).toBeInTheDocument();
    expect(screen.getByText("80.7 KB")).toBeInTheDocument();
    expect(screen.getByText(/holds 54% of all rows/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Table rows" }));
    expect(screen.getByRole("button", { name: "woocommerce_log: 44k rows" })).toBeInTheDocument();
    expect(screen.queryByText(/HPOS/)).not.toBeInTheDocument();
    expect(screen.queryByText(/differ from their posts copy/)).not.toBeInTheDocument();
  });

  it("shows a drift badge on the dashboard when wc_orders differ from their posts copy", async () => {
    mocked.wpDashboard.mockResolvedValue({ ...dashboard, hposDrift: 4 });
    render(<App />);
    expect(await screen.findByText(/4 wc_orders differ from their posts copy/)).toBeInTheDocument();
  });

  it("shows the table prefix in the dashboard header", async () => {
    render(<App />);
    expect(await screen.findByText(/· wp_$/)).toBeInTheDocument();
  });

  it("turns alerts into actions: changes queue with SQL, reviews open filtered lists", async () => {
    render(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "Set retention" }));
    expect(mocked.wpPlanChange).toHaveBeenCalledWith("s1", { type: "deleteLogsBefore", database: "db", days: 30, source: null });
    expect(within(await screen.findByRole("region", { name: "Pending changes" })).getByText("1 pending change")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Review" }));
    expect(selectWpRoute(useStore.getState())).toEqual({ screen: "list", list: "actions", params: { status: "failed" } });
  });

  it("groups failed actions by hook and expands them with their last log line", async () => {
    mocked.wpActions.mockResolvedValue({ items: [{ id: "77", hook: "woocommerce_paypal_payments_check_pui_payment_captured", status: "failed", scheduled: now - 3 * 86400, lastAttempt: now - 3 * 86400, attempts: 1, args: null, group: null, message: "PayPal API returned 500" }], hasMore: false, limit: 50, facets: [], kinds: [] });
    render(<App />);
    const group = await screen.findByRole("button", { name: /check_pui_payment_captured/ });
    expect(group).toHaveTextContent("×38");
    await userEvent.click(group);
    expect(await screen.findByText("PayPal API returned 500")).toBeInTheDocument();
    expect(mocked.wpActions).toHaveBeenCalledWith("s1", "db", expect.objectContaining({ status: "failed", kind: "woocommerce_paypal_payments_check_pui_payment_captured" }));
  });

  it("switches to the database view and back", async () => {
    render(<App />);
    expect(screen.queryByRole("navigation", { name: "Favorites" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Database/ }));
    expect(screen.getByRole("tree", { name: "Databases" })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Favorites" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /WordPress/ }));
    expect(await screen.findByText("Recent orders")).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "Favorites" })).not.toBeInTheDocument();
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

  it("lists orders with their stored status slug and gateway ID", async () => {
    useStore.getState().navigate({ screen: "list", list: "orders" });
    render(<WpWorkspace />);
    const row = await screen.findByRole("row", { name: /#3229/ });
    expect(row).toHaveTextContent("wc-processing");
    expect(within(row).getByText("ppcp-gateway")).toHaveAttribute("title", "PayPal");
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

  it("resizes the side pane by dragging the divider", async () => {
    window.innerWidth = 1600;
    useStore.getState().navigate({ screen: "list", list: "orders" });
    render(<WpWorkspace />);
    await userEvent.click(await screen.findByRole("row", { name: /#3229/ }));
    const handle = await screen.findByRole("separator", { name: "Resize detail pane" });
    fireEvent.mouseDown(handle, { button: 0, clientX: 1000 });
    fireEvent.mouseMove(window, { clientX: 950 });
    fireEvent.mouseMove(window, { clientX: 900 });
    fireEvent.mouseUp(window, { clientX: 880 });
    expect(handle).toHaveAttribute("aria-valuenow", "680");
  });

  it("lists users with ID, login and stored role slugs", async () => {
    window.innerWidth = 1200;
    mocked.wpUsers.mockResolvedValue({
      items: [{ id: "12", login: "jane", email: "jane@shop.test", name: "Jane Doe", registered: now - 86400, roles: ["customer"], orderCount: 1, totalSpent: "€49.90", lastOrder: now - 3600, avatar: null }],
      hasMore: false,
      limit: 50,
      facets: [],
      kinds: [{ value: "customer", label: "customer", count: 1 }],
    });
    useStore.getState().navigate({ screen: "list", list: "users" });
    render(<WpWorkspace />);
    const row = await screen.findByRole("row", { name: /#12 jane/ });
    expect(within(row).getByText("customer")).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "customer (1)" })).toBeInTheDocument();
    await userEvent.click(row);
    expect(selectWpRoute(useStore.getState())).toEqual({ screen: "entity", entity: "user", id: "12" });
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
    expect(tab.kind === "table" && [tab.table, tab.where]).toEqual(["wp_woocommerce_log", [{ column: "log_id", value: "1732" }]]);
    expect(useStore.getState().mode).toBe("db");
  });

  it("names payment facts by their stored keys and opens item meta rows", async () => {
    useStore.getState().openEntity("order", "4711");
    render(<WpWorkspace />);
    expect(within(await screen.findByRole("region", { name: "Payment" })).getByText("_ppcp_paypal_order_id")).toBeInTheDocument();
    expect(within(screen.getByRole("region", { name: "Payment" })).getByText("payment_method")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "item #1" }));
    expect(useStore.getState().tabs.at(-1)).toMatchObject({ table: "wp_woocommerce_order_itemmeta", where: [{ column: "order_item_id", value: "1" }] });
  });

  it("queues an order status change from the header", async () => {
    useStore.getState().openEntity("order", "4711");
    render(<WpWorkspace />);
    await userEvent.selectOptions(await screen.findByLabelText("Change status"), "completed");
    expect(mocked.wpPlanChange).toHaveBeenCalledWith("s1", { type: "setOrderStatus", database: "db", id: "4711", status: "completed" });
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
  });
});

describe("List filters in the route", () => {
  const gateways = [{ value: "ppcp-gateway", label: "PayPal", count: 2 }];
  const page = (patch: object = {}) => ({
    items: [order("3229"), order("3227", "wc-failed")],
    hasMore: false,
    limit: 50,
    facets: [
      { value: "wc-processing", label: "Processing", count: 1 },
      { value: "wc-failed", label: "Failed", count: 1 },
    ],
    kinds: gateways,
    ...patch,
  });

  beforeEach(() => {
    vi.clearAllMocks();
    wpStore();
    mocked.wpBrowser.mockResolvedValue([]);
    mocked.wpOrders.mockResolvedValue(page());
    mocked.wpView.mockResolvedValue(orderView);
  });

  it("keeps the status chip after opening an order and going back", async () => {
    window.innerWidth = 1000;
    useStore.getState().navigate({ screen: "list", list: "orders" });
    render(<WpWorkspace />);
    await userEvent.click(await screen.findByRole("button", { name: /Failed 1/ }));
    await userEvent.click(await screen.findByRole("row", { name: /#3229/ }));
    expect(selectWpRoute(useStore.getState())).toMatchObject({ screen: "entity" });
    await userEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(await screen.findByRole("button", { name: /Failed 1/ })).toHaveClass("on");
    expect(mocked.wpOrders).toHaveBeenLastCalledWith("s1", "db", "shop_order", expect.objectContaining({ status: "wc-failed" }));
  });

  it("keeps the gateway selection after opening an order and going back", async () => {
    window.innerWidth = 1000;
    useStore.getState().navigate({ screen: "list", list: "orders" });
    render(<WpWorkspace />);
    await userEvent.selectOptions(await screen.findByLabelText("Gateway"), "ppcp-gateway");
    await userEvent.click(await screen.findByRole("row", { name: /#3229/ }));
    await userEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(await screen.findByLabelText("Gateway")).toHaveValue("ppcp-gateway");
    expect(mocked.wpOrders).toHaveBeenLastCalledWith("s1", "db", "shop_order", expect.objectContaining({ kind: "ppcp-gateway" }));
  });

  it("fetches once when a status chip is picked on page 2, starting at offset 0", async () => {
    mocked.wpOrders.mockResolvedValue(page({ hasMore: true }));
    useStore.getState().navigate({ screen: "list", list: "orders" });
    render(<WpWorkspace />);
    await userEvent.click(await screen.findByRole("button", { name: "Next page" }));
    await screen.findByText("51–52");
    mocked.wpOrders.mockClear();
    await userEvent.click(screen.getByRole("button", { name: /Failed 1/ }));
    await vi.waitFor(() => expect(mocked.wpOrders).toHaveBeenLastCalledWith("s1", "db", "shop_order", expect.objectContaining({ status: "wc-failed", offset: 0 })));
    expect(mocked.wpOrders).toHaveBeenCalledTimes(1);
  });

  it("does not open or select a row with ArrowDown on a narrow window", async () => {
    window.innerWidth = 1000;
    useStore.getState().navigate({ screen: "list", list: "orders" });
    render(<WpWorkspace />);
    (await screen.findByRole("grid", { name: "orders list" })).focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(selectWpRoute(useStore.getState())).toEqual({ screen: "list", list: "orders" });
    expect(screen.queryAllByRole("row", { selected: true })).toHaveLength(0);
  });

  it("starts the content list on the all-types chip and filters by a picked type", async () => {
    mocked.wpContent.mockResolvedValue({
      items: [],
      hasMore: false,
      limit: 50,
      facets: [],
      kinds: [
        { value: "post", label: "Post", count: 5 },
        { value: "page", label: "Page", count: 2 },
      ],
    });
    useStore.getState().navigate({ screen: "list", list: "content" });
    render(<WpWorkspace />);
    expect(await screen.findByRole("button", { name: /All types/ })).toHaveClass("on");
    expect(mocked.wpContent).toHaveBeenLastCalledWith("s1", "db", expect.objectContaining({ kind: null }));
    await userEvent.click(screen.getByRole("button", { name: /^Page/ }));
    await vi.waitFor(() => expect(mocked.wpContent).toHaveBeenLastCalledWith("s1", "db", expect.objectContaining({ kind: "page" })));
    expect(screen.getByRole("button", { name: /^Page/ })).toHaveClass("on");
    expect(screen.getByRole("button", { name: /All types/ })).not.toHaveClass("on");
  });

  it("labels the orders gateway select's first option 'All gateways'", async () => {
    useStore.getState().navigate({ screen: "list", list: "orders" });
    render(<WpWorkspace />);
    const select = await screen.findByLabelText("Gateway");
    expect(within(select).getAllByRole("option")[0]).toHaveTextContent("All gateways");
  });

  it("drops the old rows while another database's list is loading", async () => {
    useStore.getState().navigate({ screen: "list", list: "orders" });
    render(<WpWorkspace />);
    await screen.findByRole("row", { name: /#3229/ });
    mocked.wpOrders.mockReturnValue(new Promise(() => {}));
    act(() => {
      useStore.setState((s) => ({
        sessions: s.sessions.map((x) => ({ ...x, database: "other" })),
        wp: { ...s.wp, [`${session.id}/other`]: wpInfo },
      }));
    });
    expect(await screen.findByText("Loading…")).toBeInTheDocument();
    expect(screen.queryByRole("row", { name: /#3229/ })).not.toBeInTheDocument();
    expect(mocked.wpOrders).toHaveBeenLastCalledWith("s1", "other", "shop_order", expect.anything());
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

  it("ranks commands by how well their title matches", async () => {
    useStore.getState().setPaletteOpen(true);
    render(<CommandPalette />);
    const input = screen.getByLabelText("Search everything");
    const titles = () => screen.getAllByRole("option").map((o) => o.getAttribute("data-stage-label"));
    await userEvent.type(input, ">filter");
    expect(titles()).toEqual(["Filter tables", "Value filter (WHERE)"]);
    await userEvent.clear(input);
    await userEvent.type(input, ">zoom");
    expect(titles()).toEqual(["Zoom in", "Zoom out", "Reset zoom"]);
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

  it("plans WordPress changes through wpPlanChange, applies them by id in one batch and undoes that batch", async () => {
    mocked.wpPlanChange
      .mockResolvedValueOnce({ id: 11, description: "planned retryAction", statements: ["-- retryAction"], canUndo: true })
      .mockResolvedValueOnce({ id: 12, description: "planned setAutoload", statements: ["-- setAutoload"], canUndo: true });
    mocked.applyChanges.mockResolvedValueOnce({ batchId: 7, affected: 2, canUndo: true });
    await useStore.getState().queueChange({ type: "retryAction", database: "db", id: "1" });
    await useStore.getState().queueChange({ type: "setAutoload", database: "db", option: "x", on: false });
    render(<PendingTray />);
    expect(screen.getByText("2 pending changes")).toBeInTheDocument();
    expect(mocked.planChange).not.toHaveBeenCalled();
    await userEvent.click(screen.getAllByRole("button", { name: "Show SQL" })[0]);
    expect(screen.getByText("-- retryAction")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Apply/ }));
    expect(mocked.applyChanges).toHaveBeenCalledWith("s1", [11, 12]);
    await userEvent.click(await screen.findByRole("button", { name: /Undo last apply/ }));
    expect(mocked.undoBatch).toHaveBeenCalledWith("s1", 7);
  });

  it("discards a single change on the server and removes it from the tray", async () => {
    mocked.wpPlanChange.mockResolvedValueOnce({ id: 21, description: "planned retryAction", statements: ["-- retryAction"], canUndo: true });
    await useStore.getState().queueChange({ type: "retryAction", database: "db", id: "1" });
    render(<PendingTray />);
    await userEvent.click(screen.getByRole("button", { name: /Discard planned retryAction/ }));
    expect(mocked.discardChange).toHaveBeenCalledWith("s1", 21);
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
    mocked.wpSnapshots.mockResolvedValue([{ id: "a", label: "order #4711", scope: { entity: "order", id: "4711", includeRelated: true, optionPattern: null, includeActions: true }, created: new Date(NOW).toISOString(), paths: 40 }]);
    mocked.wpSnapshotDiff.mockResolvedValue([{ path: "order#4711/field/status", before: "wc-pending", after: "wc-processing" }]);
    render(<SnapshotsScreen />);
    await userEvent.click(await screen.findByRole("button", { name: /Diff now/ }));
    const row = await screen.findByRole("row", { name: /order#4711\/field\/status/ });
    expect(within(row).getByText("wc-pending")).toBeInTheDocument();
    expect(within(row).getByText("wc-processing")).toBeInTheDocument();
  });

  describe("deleting a snapshot", () => {
    const snapshot = { id: "a", label: "order #4711", scope: { entity: "order", id: "4711", includeRelated: true, optionPattern: null, includeActions: true }, created: new Date(NOW).toISOString(), paths: 40 };
    const renderSnapshots = () => {
      useStore.getState().navigate({ screen: "snapshots" });
      render(
        <>
          <WpWorkspace />
          <Dialogs />
          <Toasts />
        </>,
      );
    };

    beforeEach(() => {
      mocked.wpSnapshotDelete.mockResolvedValue(undefined);
      mocked.wpSnapshots.mockResolvedValue([snapshot]);
    });

    it("asks for confirmation and deletes only after Delete is clicked", async () => {
      renderSnapshots();
      await userEvent.click(await screen.findByRole("button", { name: "Delete snapshot order #4711" }));
      expect(await screen.findByText(/Delete the snapshot "order #4711"\?/)).toBeInTheDocument();
      expect(mocked.wpSnapshotDelete).not.toHaveBeenCalled();
      mocked.wpSnapshots.mockResolvedValue([]);
      await userEvent.click(screen.getByRole("button", { name: "Delete" }));
      expect(mocked.wpSnapshotDelete).toHaveBeenCalledWith("a");
      expect(await screen.findByText("No snapshots yet.")).toBeInTheDocument();
    });

    it("shows an error toast when the delete is rejected", async () => {
      mocked.wpSnapshotDelete.mockRejectedValueOnce({ kind: "io", message: "disk full", code: null });
      renderSnapshots();
      await userEvent.click(await screen.findByRole("button", { name: "Delete snapshot order #4711" }));
      await userEvent.click(await screen.findByRole("button", { name: "Delete" }));
      expect(await screen.findByText("Could not delete the snapshot: disk full")).toBeInTheDocument();
      expect(screen.getByText("order #4711")).toBeInTheDocument();
    });

    it("removes the diff card of a snapshot once it is deleted", async () => {
      mocked.wpSnapshotDiff.mockResolvedValue([{ path: "order#4711/field/status", before: "wc-pending", after: "wc-processing" }]);
      renderSnapshots();
      await userEvent.click(await screen.findByRole("button", { name: /Diff now/ }));
      expect(await screen.findByText("Changes since snapshot")).toBeInTheDocument();
      await userEvent.click(screen.getByRole("button", { name: "Delete snapshot order #4711" }));
      mocked.wpSnapshots.mockResolvedValue([]);
      await userEvent.click(await screen.findByRole("button", { name: "Delete" }));
      await vi.waitFor(() => expect(screen.queryByText("Changes since snapshot")).not.toBeInTheDocument());
    });
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

describe("Rust-provided labels and targets", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    wpStore();
    mocked.wpBrowser.mockResolvedValue([]);
  });

  it("opens the orders list with the search of a customer's related chip", async () => {
    mocked.wpView.mockResolvedValue({
      view: "customer",
      doc: doc({ entity: "user", label: "User", id: "12", table: "wp_users", idColumn: "ID" }),
      name: "Jane Doe",
      login: "jane",
      email: "jane@x",
      avatar: null,
      roles: ["customer"],
      registered: now - 86400,
      lastActive: null,
      billing: null,
      shipping: null,
      orderCount: 2,
      totalSpent: null,
      averageOrder: null,
      orders: [],
      subscriptions: [],
      tokens: [],
      session: null,
      related: [{ label: "Orders by email", link: null, count: 2, target: { kind: "list", list: "orders", params: { search: "jane@x" } } }],
    });
    useStore.getState().openEntity("user", "12");
    render(<WpWorkspace />);
    await userEvent.click(await screen.findByRole("button", { name: /Orders by email/ }));
    expect(selectWpRoute(useStore.getState())).toEqual({ screen: "list", list: "orders", params: { search: "jane@x" } });
    await vi.waitFor(() => expect(mocked.wpOrders).toHaveBeenLastCalledWith("s1", "db", "shop_order", expect.objectContaining({ search: "jane@x" })));
  });

  it("labels the action histogram legend with the status names from Rust", async () => {
    mocked.wpActionHistogram.mockResolvedValue({
      days: [{ date: "2026-10-15", completed: 3, processing: 1, failed: 1, other: 0 }],
      labels: { completed: "complete", processing: "in-progress", failed: "failed", other: "pending" },
    });
    useStore.getState().navigate({ screen: "list", list: "actions" });
    render(<WpWorkspace />);
    expect(await screen.findByText("in-progress")).toBeInTheDocument();
    expect(screen.getByText("pending")).toBeInTheDocument();
    expect(screen.queryByText("processing")).not.toBeInTheDocument();
  });
});
