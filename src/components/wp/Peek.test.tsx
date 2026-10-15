import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api } from "../../api";
import { useStore } from "../../state/store";
import { wpStore } from "../../test/wpFixtures";
import { FindScreen } from "./FindScreen";
import { LinkChip } from "./LinkChip";
import { clearPeekCache, PEEK_DELAY_MS } from "./Peek";

const mocked = vi.mocked(api, true);

describe("Peek cards", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearPeekCache();
    wpStore();
    vi.useFakeTimers({ shouldAdvanceTime: true });
  });
  afterEach(() => vi.useRealTimers());

  it("shows key facts after hovering a link chip and caches them", async () => {
    mocked.wpPeek.mockResolvedValue([
      { label: "Status", value: "wc-processing", kind: "status" },
      { label: "Total", value: "€49.90", kind: "money" },
      { label: "Customer", value: "jane@shop.test", kind: "text" },
    ]);
    render(<LinkChip link={{ entity: "order", id: "4711", label: "Order #4711", image: null }} />);
    const chip = screen.getByRole("button", { name: "Open order 4711" });
    fireEvent.mouseEnter(chip.parentElement!);
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    await act(() => vi.advanceTimersByTimeAsync(PEEK_DELAY_MS + 10));
    const card = await screen.findByRole("tooltip");
    expect(card).toHaveTextContent("Processing");
    expect(card).toHaveTextContent("€49.90");
    expect(card).toHaveTextContent("jane@shop.test");
    expect(mocked.wpPeek).toHaveBeenCalledWith("s1", "db", "order", "4711");

    fireEvent.mouseLeave(chip.parentElement!);
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    fireEvent.mouseEnter(chip.parentElement!);
    await act(() => vi.advanceTimersByTimeAsync(PEEK_DELAY_MS + 10));
    await screen.findByRole("tooltip");
    expect(mocked.wpPeek).toHaveBeenCalledTimes(1);
  });

  it("does not peek on a quick pass-over, and a click still navigates", async () => {
    render(<LinkChip link={{ entity: "order", id: "4711", label: "Order #4711", image: null }} />);
    const chip = screen.getByRole("button", { name: "Open order 4711" });
    fireEvent.mouseEnter(chip.parentElement!);
    await act(() => vi.advanceTimersByTimeAsync(100));
    fireEvent.mouseLeave(chip.parentElement!);
    await act(() => vi.advanceTimersByTimeAsync(PEEK_DELAY_MS));
    expect(mocked.wpPeek).not.toHaveBeenCalled();
    await userEvent.click(chip);
    expect(useStore.getState().wpNav.stack.at(-1)).toEqual({ screen: "entity", entity: "order", id: "4711" });
  });
});

describe("Find everywhere", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    wpStore();
  });

  it("groups hits by source and opens entities or rows", async () => {
    mocked.wpFindValue.mockResolvedValue([
      { source: "Order meta", table: "wp_wc_orders_meta", idColumn: "id", count: 1, hits: [{ rowId: "9", label: "order #4711 · _ppcp_paypal_order_id", entity: "order", entityId: "4711", column: "meta_value" }] },
      { source: "WooCommerce log", table: "wp_woocommerce_log", idColumn: "log_id", count: 120, hits: [{ rowId: "55", label: "woocommerce-paypal-payments · PATCH …", entity: null, entityId: null, column: "message" }] },
    ]);
    render(<FindScreen value="66M12696JX411763D" />);
    expect(await screen.findByText("121 matches in 2 places")).toBeInTheDocument();
    expect(screen.getByText("Order meta")).toBeInTheDocument();
    expect(screen.getByText(/120 in wp_woocommerce_log · showing 1/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "order #4711 · _ppcp_paypal_order_id" }));
    expect(useStore.getState().wpNav.stack.at(-1)).toEqual({ screen: "entity", entity: "order", id: "4711" });
    await userEvent.click(screen.getAllByRole("button", { name: /row/ }).at(-1)!);
    const tab = useStore.getState().tabs.at(-1)!;
    expect(tab.kind === "table" && [tab.table, tab.filter]).toEqual(["wp_woocommerce_log", "`log_id` = 55"]);
  });
});
