import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { address } from "../lib/address";
import { useStore } from "../state/store";
import { connectedStore, queryTab, tableTab } from "../test/fixtures";
import { activeObject, orderAddress } from "../test/objectFixtures";
import { Path } from "./Path";

const path = () => screen.getByRole("navigation", { name: "Path" });

describe("Path", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [] });
  });

  it("is absent for a query tab", () => {
    connectedStore({ tabs: [queryTab()] });
    render(<Path />);
    expect(screen.queryByRole("navigation", { name: "Path" })).not.toBeInTheDocument();
  });

  describe("table tab", () => {
    it("shows the table name as the current location", () => {
      connectedStore({ tabs: [tableTab()] });
      render(<Path />);
      expect(within(path()).getByText("wp_posts")).toBeInTheDocument();
      expect(within(path()).queryByText("(filtered)")).not.toBeInTheDocument();
      expect(within(path()).queryByRole("button")).not.toBeInTheDocument();
    });

    it("marks a table filtered by columns", () => {
      connectedStore({ tabs: [tableTab({ where: [{ column: "post_type", value: "page" }] })] });
      render(<Path />);
      expect(within(path()).getByText("(filtered)")).toBeInTheDocument();
    });

    it("marks a table filtered by a typed filter", () => {
      connectedStore({ tabs: [tableTab({ filter: "post_status = 'draft'" })] });
      render(<Path />);
      expect(within(path()).getByText("(filtered)")).toBeInTheDocument();
    });
  });

  describe("object tab", () => {
    it("leads with a crumb that opens the whole table in the same tab", async () => {
      render(<Path />);
      act(() => useStore.getState().openObject(orderAddress));
      const { id } = activeObject();

      await userEvent.click(within(path()).getByRole("button", { name: "Open table wp_wc_orders" }));

      expect(useStore.getState().tabs).toHaveLength(1);
      expect(useStore.getState().tabs[0]).toMatchObject({ id, kind: "table", table: "wp_wc_orders", where: [] });
    });

    it("shows the table crumb of the root row while a drilled row is active", () => {
      render(<Path />);
      act(() => {
        useStore.getState().openObject(orderAddress);
        useStore.getState().drill(activeObject().id, 0, address("shop", "db", { kind: "row", table: "wp_users", key: "12" }));
      });
      expect(within(path()).getByRole("button", { name: "Open table wp_wc_orders" })).toBeInTheDocument();
      expect(within(path()).getByRole("button", { name: "wp_wc_orders #4711" })).toBeInTheDocument();
    });
  });
});
