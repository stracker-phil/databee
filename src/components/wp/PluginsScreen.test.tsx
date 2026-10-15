import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api, type LensReport } from "../../api";
import { selectWpRoute, useStore } from "../../state/store";
import { wpStore } from "../../test/wpFixtures";
import { Dialogs } from "../Dialogs";
import { PendingTray } from "../PendingTray";
import { PluginsScreen } from "./PluginsScreen";

const mocked = vi.mocked(api, true);
const now = Math.round(Date.now() / 1000);

const report: LensReport = {
  activePlugins: ["woocommerce"],
  plugins: [
    {
      slug: "woocommerce", name: "WooCommerce", active: true, optionCount: 341, optionBytes: 48_000, autoloadBytes: 30_000, transientCount: 3, transientBytes: 900,
      meta: [{ table: "postmeta", keys: 20, rows: 900, bytes: 40_000 }], hookCount: 2000, logEntries: 100, logBytes: 5000, tables: [], orders: 0, lastWritten: now - 3600, totalBytes: 94_000,
      keys: [{ kind: "option", name: "woocommerce_currency", count: 1, bytes: 3, autoload: true, lastWritten: null, plugin: "woocommerce", confidence: "exact", source: "registry" }],
    },
    {
      slug: "woocommerce-paypal-payments", name: "PayPal Payments", active: false, optionCount: 4, optionBytes: 2400, autoloadBytes: 2400, transientCount: 1, transientBytes: 100,
      meta: [{ table: "wc_orders_meta", keys: 6, rows: 1200, bytes: 60_000 }], hookCount: 40, logEntries: 4900, logBytes: 900_000, tables: [{ kind: "table", name: "ppcp_things", count: 10, bytes: 16384, autoload: false, lastWritten: null }], orders: 12, lastWritten: now - 86400, totalBytes: 978_884,
      keys: [
        { kind: "option", name: "woocommerce-ppcp-settings", count: 1, bytes: 2400, autoload: true, lastWritten: null, plugin: "woocommerce-paypal-payments", confidence: "exact", source: "registry" },
        { kind: "meta", table: "wc_orders_meta", name: "_ppcp_paypal_order_id", count: 1200, bytes: 60_000, autoload: false, lastWritten: null, plugin: "woocommerce-paypal-payments", confidence: "exact", source: "registry" },
        { kind: "hook", name: "ppcp_check_payment_status", count: 40, bytes: 0, autoload: false, lastWritten: now - 86400, plugin: "woocommerce-paypal-payments", confidence: "likely", source: "prefix" },
        { kind: "table", name: "ppcp_things", count: 10, bytes: 16384, autoload: false, lastWritten: null, plugin: "woocommerce-paypal-payments", confidence: "exact", source: "registry" },
      ],
    },
  ],
  unattributed: [{ kind: "option", name: "zzz_mystery_setting", count: 1, bytes: 1, autoload: false, lastWritten: null, plugin: null, confidence: null, source: null }],
};

describe("Plugins lens", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    wpStore();
    mocked.wpLens.mockResolvedValue(report);
  });

  it("lists plugins with storage totals and flags leftover data", async () => {
    render(<PluginsScreen />);
    const grid = await screen.findByRole("grid", { name: "plugins list" });
    const paypal = within(grid).getByRole("row", { name: /PayPal Payments/ });
    expect(paypal).toHaveTextContent("not active");
    expect(paypal).toHaveTextContent("955.9 KB");
    expect(within(grid).getByRole("row", { name: /^WooCommerce/ })).not.toHaveTextContent("not active");
    const leftover = screen.getByRole("heading", { name: "Leftover data" }).closest(".card")!;
    expect(within(leftover as HTMLElement).getByText("2 keys")).toBeInTheDocument();
  });

  it("queues a reviewed cleanup that only names options, transients and meta keys", async () => {
    render(
      <>
        <PluginsScreen />
        <PendingTray />
      </>,
    );
    await userEvent.click(await screen.findByRole("button", { name: /Queue cleanup/ }));
    expect(mocked.planChange).toHaveBeenCalledWith("s1", {
      type: "deletePluginData",
      database: "db",
      slug: "woocommerce-paypal-payments",
      keys: ["option:woocommerce-ppcp-settings", "meta:wc_orders_meta:_ppcp_paypal_order_id"],
    });
    expect(await screen.findByText("1 pending change")).toBeInTheDocument();
    expect(mocked.applyChanges).not.toHaveBeenCalled();
  });

  it("opens a plugin's keys with confidence markers and lets Likely keys be reassigned", async () => {
    useStore.getState().navigate({ screen: "list", list: "plugins", params: { slug: "woocommerce-paypal-payments", name: "PayPal Payments" } });
    render(
      <>
        <PluginsScreen params={{ slug: "woocommerce-paypal-payments" }} />
        <Dialogs />
      </>,
    );
    expect(await screen.findByRole("heading", { name: "PayPal Payments" })).toBeInTheDocument();
    expect(screen.getByText("Scheduled hooks")).toBeInTheDocument();
    const hook = screen.getByRole("row", { name: /ppcp_check_payment_status/ });
    expect(within(hook).getByText("likely")).toBeInTheDocument();
    await userEvent.click(within(hook).getByRole("button", { name: /reassign/ }));
    const input = await screen.findByRole("textbox");
    await userEvent.clear(input);
    await userEvent.type(input, "my-plugin{Enter}");
    expect(mocked.wpPluginAssign).toHaveBeenCalledWith("hook:ppcp_check_payment_status", "my-plugin");
    expect(screen.getByText("never dropped automatically")).toBeInTheDocument();
  });

  it("assigns unattributed keys from the overview", async () => {
    render(
      <>
        <PluginsScreen />
        <Dialogs />
      </>,
    );
    const row = await screen.findByRole("row", { name: /zzz_mystery_setting/ });
    await userEvent.click(within(row).getByRole("button", { name: "assign…" }));
    await userEvent.type(await screen.findByRole("textbox"), "acme{Enter}");
    expect(mocked.wpPluginAssign).toHaveBeenCalledWith("option:zzz_mystery_setting", "acme");
    expect(selectWpRoute(useStore.getState())).toEqual({ screen: "dashboard" });
  });
});
