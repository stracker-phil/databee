import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api } from "../../api";
import { useStore } from "../../state/store";
import { field, wpStore } from "../../test/wpFixtures";
import { SettingsScreen } from "./Settings";

const mocked = vi.mocked(api, true);

describe("SettingsScreen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    wpStore();
    mocked.wpOptions.mockResolvedValue({
      table: "wp_options",
      totalCount: 3,
      autoloadBytes: 1200,
      groups: [{ prefix: "woocommerce", label: "woocommerce*", count: 2, bytes: 1000, autoloadCount: 2, autoloadBytes: 1000 }],
      largestAutoload: [{ name: "woocommerce-ppcp-data-common", bytes: 900 }],
      transients: [],
    });
  });

  it("labels autoload groups by their literal name prefix", async () => {
    render(<SettingsScreen />);
    expect(await screen.findByText("woocommerce*")).toBeInTheDocument();
  });

  it("opens one of the largest autoloaded options as its row in the Database view", async () => {
    render(<SettingsScreen />);
    await userEvent.click(await screen.findByRole("button", { name: /woocommerce-ppcp-data-common/ }));
    expect(useStore.getState().mode).toBe("db");
    expect(useStore.getState().tabs.at(-1)).toMatchObject({ kind: "table", table: "wp_options", where: [{ column: "option_name", value: "woocommerce-ppcp-data-common" }] });
  });

  describe("option group", () => {
    beforeEach(() => {
      mocked.wpOptionsGroup.mockResolvedValue([
        field("woocommerce_currency", "EUR", { autoload: { value: "yes", on: true } }),
        field("woocommerce_cache", "x", { autoload: { value: "off", on: false } }),
        field("woocommerce_plain", "y"),
      ]);
    });

    it("shows each option's autoload state from the server", async () => {
      render(<SettingsScreen params={{ prefix: "woocommerce" }} />);
      expect(await screen.findByRole("button", { name: "woocommerce_currency" })).toHaveAttribute("title", "Queue autoload off for woocommerce_currency");
      expect(screen.getByRole("button", { name: "woocommerce_cache" })).toHaveAttribute("title", "Queue autoload on for woocommerce_cache");
      expect(screen.getByRole("button", { name: "woocommerce_plain" })).toHaveAttribute("title", "Queue autoload on for woocommerce_plain");
    });

    it("queues the opposite autoload state when a pill is clicked", async () => {
      render(<SettingsScreen params={{ prefix: "woocommerce" }} />);
      await userEvent.click(await screen.findByRole("button", { name: "woocommerce_currency" }));
      expect(mocked.wpPlanChange).toHaveBeenCalledWith("s1", { type: "setAutoload", database: "db", option: "woocommerce_currency", on: false });
    });
  });
});
