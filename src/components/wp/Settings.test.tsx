import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api } from "../../api";
import { useStore } from "../../state/store";
import { wpStore } from "../../test/wpFixtures";
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
    expect(useStore.getState().tabs.at(-1)).toMatchObject({ kind: "table", table: "wp_options", filter: "`option_name` = 'woocommerce-ppcp-data-common'" });
  });
});
