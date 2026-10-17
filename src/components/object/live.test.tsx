import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import App from "../../App";
import { api, type Address, type DocDelta, type RowMark, type SnapshotInfo, type ValueChange } from "../../api";
import { installCommands, runCommand } from "../../commands";
import { formatAddress } from "../../lib/address";
import { useStore } from "../../state/store";
import { connectedStore, num, table, text, value } from "../../test/fixtures";
import { activeObject, browseOrderTables, objectDocument, orderAddress, orderGroups, source } from "../../test/objectFixtures";
import { settings } from "../../test/settings";

const mocked = vi.mocked(api, true);

const ORDER_URL = "databee://shop/db/wp_wc_orders/4711";
const tables = ["wp_wc_orders", "wp_wc_orders_meta"].map((name) => table(name));

const statusSource = source("wp_wc_orders", "status", "4711");
const totalSource = source("wp_wc_orders", "total_amount", "4711");

const statusChange: ValueChange = { group: "own", label: "status", source: statusSource, oldSource: statusSource, old: text("wc-pending"), new: text("wc-processing") };
const totalChange: ValueChange = { group: "own", label: "total_amount", source: totalSource, oldSource: totalSource, old: num("99.00"), new: num("119.00") };

const metaMark = (id: string, key: string, raw: string): RowMark => ({
  group: "meta",
  table: "wp_wc_orders_meta",
  key: [{ column: "id", value: id }],
  label: key,
  value: text(raw),
  fields: [
    ["meta_key", value({ raw: text(key), source: source("wp_wc_orders_meta", "meta_key", id) })],
    ["meta_value", value({ raw: text(raw), source: source("wp_wc_orders_meta", "meta_value", id) })],
  ],
});

const delta = (patch: Partial<DocDelta>): DocDelta => ({ changed: [], added: [], removed: [], ...patch });
const rowOf = (label: string) => {
  const rows = screen
    .getAllByText(label)
    .map((el) => el.closest<HTMLElement>(".row")!)
    .filter((row) => !row.classList.contains("change-line"));
  expect(rows).toHaveLength(1);
  return rows[0];
};

async function openOrder(address: Address = orderAddress) {
  render(<App />);
  await screen.findByRole("tree");
  act(() => useStore.getState().openObject(address));
  await screen.findAllByRole("region", { name: "Object header" });
}

async function focusStatus() {
  await openOrder();
  await userEvent.click(screen.getByText("wc-processing"));
}

const advance = (ms: number) => act(() => vi.advanceTimersByTimeAsync(ms));

async function goLive(seconds = 2) {
  await focusStatus();
  await waitFor(() => expect(useStore.getState().settings?.liveIntervalSecs).toBe(seconds));
  vi.useFakeTimers({ shouldAdvanceTime: true });
  vi.setSystemTime(new Date(2025, 0, 2, 14, 2, 10));
  await userEvent.keyboard("w");
  await screen.findByRole("button", { name: /● live/ });
  await advance(0);
}

const pollOnce = (patch: Partial<DocDelta>) => {
  mocked.watchPoll.mockResolvedValueOnce({ delta: delta(patch), document: null });
  return advance(2000);
};

const snapshot = (patch: Partial<SnapshotInfo> = {}): SnapshotInfo => ({
  id: "snap-1",
  label: "before deploy",
  connection: "shop",
  database: "db",
  address: ORDER_URL,
  created: Math.floor(Date.now() / 1000) - 7200,
  values: 5,
  ...patch,
});

describe("live mode and change marks", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocked.getSettings.mockResolvedValue(settings());
    connectedStore({ tabs: [], tables });
    installCommands();
    mocked.document.mockResolvedValue(objectDocument());
    mocked.browseTable.mockImplementation(browseOrderTables);
  });

  afterEach(() => vi.useRealTimers());

  describe("w", () => {
    it("starts watching the row, polls after the live interval and stops on the next w", async () => {
      mocked.getSettings.mockResolvedValue(settings({ liveIntervalSecs: 5 }));
      await goLive(5);
      expect(mocked.watchStart).toHaveBeenCalledWith("s1", ORDER_URL);
      expect(screen.getByRole("button", { name: "● live 5 s" })).toBeInTheDocument();

      await advance(3000);
      expect(mocked.watchPoll).not.toHaveBeenCalled();
      await advance(2500);
      expect(mocked.watchPoll).toHaveBeenCalledWith("s1", "w1");

      await userEvent.keyboard("w");
      expect(mocked.watchStop).toHaveBeenCalledWith("s1", "w1");
      expect(screen.queryByRole("button", { name: /● live/ })).not.toBeInTheDocument();
      await advance(20000);
      expect(mocked.watchPoll).toHaveBeenCalledTimes(1);
    });

    it("does not fire w, x or s while typing in an input", async () => {
      await focusStatus();
      await userEvent.keyboard("e");
      const input = await screen.findByLabelText("Edit value");
      await userEvent.type(input, "wxs");
      expect(input).toHaveValue("wc-processingwxs");
      expect(mocked.watchStart).not.toHaveBeenCalled();
      expect(screen.queryByRole("textbox", { name: /^Snapshot of/ })).not.toBeInTheDocument();
    });

    it("stops live mode and says why when a poll fails", async () => {
      await goLive();
      mocked.watchPoll.mockRejectedValueOnce({ kind: "invalid_input", message: "Lost connection", code: null });
      await advance(2000);
      expect(await screen.findByText("Live mode stopped: Lost connection")).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /● live/ })).not.toBeInTheDocument();
    });

    it("stops live mode when the status bar chip is clicked", async () => {
      await goLive();
      await userEvent.click(screen.getByRole("button", { name: "● live 2 s" }));
      expect(mocked.watchStop).toHaveBeenCalledWith("s1", "w1");
      expect(screen.queryByRole("button", { name: /● live/ })).not.toBeInTheDocument();
    });
  });

  describe("marks on values", () => {
    it("flashes a changed value, keeps its old value next to it and marks its row", async () => {
      await goLive();
      await pollOnce({ changed: [statusChange] });
      const row = rowOf("wc-processing");
      expect(row).toHaveClass("chg");
      expect(screen.getByText("wc-processing").closest(".flash")).not.toBeNull();
      expect(within(row).getByText("wc-pending")).toHaveClass("old");
      expect(rowOf("119.00")).not.toHaveClass("chg");
    });

    it("stops flashing a value that changed more than 1.6 s ago and keeps its old value", async () => {
      await goLive();
      await pollOnce({ changed: [statusChange] });
      await pollOnce({ changed: [totalChange] });
      expect(screen.getByText("119.00").closest(".flash")).not.toBeNull();
      expect(screen.getByText("wc-processing").closest(".flash")).toBeNull();
      expect(within(rowOf("wc-processing")).getByText("wc-pending")).toHaveClass("old");
    });

    it("shows no row as the old value of a new meta value", async () => {
      await goLive();
      await pollOnce({ added: [metaMark("91", "_paid_via", "bacs")] });
      const row = rowOf("bacs");
      expect(row).toHaveClass("chg");
      expect(within(row).getByText("no row")).toHaveClass("old");
    });

    it("keeps a removed row struck through until x", async () => {
      await goLive();
      await pollOnce({ removed: [metaMark("93", "_old_flag", "1")] });
      expect(rowOf("_old_flag")).toHaveClass("removed");
      await advance(2000);
      expect(rowOf("_old_flag")).toHaveClass("removed");

      await userEvent.keyboard("x");
      expect(screen.queryByText("_old_flag")).not.toBeInTheDocument();
    });

    it("clears every mark and the change list on x while live mode stays on", async () => {
      await goLive();
      await pollOnce({ changed: [statusChange] });
      await userEvent.keyboard("x");
      expect(rowOf("wc-processing")).not.toHaveClass("chg");
      expect(screen.queryByText("wc-pending")).not.toBeInTheDocument();
      expect(screen.queryByRole("region", { name: "Changes" })).not.toBeInTheDocument();
      expect(screen.getByRole("button", { name: "● live 2 s" })).toBeInTheDocument();
    });
  });

  describe("change list", () => {
    it("lists the change with time, table, label and old to new, and says what it is since", async () => {
      await goLive();
      await pollOnce({ changed: [statusChange] });
      const list = screen.getByRole("region", { name: "Changes" });
      expect(within(list).getByText(/since live started/)).toHaveTextContent("in key order within one poll");
      const line = within(list).getByRole("button", { name: /status/ });
      expect(line).toHaveTextContent("14:02:12");
      expect(line).toHaveTextContent("wp_wc_orders");
      expect(line).toHaveTextContent("wc-pending → wc-processing");
    });

    it("lists a new meta row as no row to its value and a removed one as its value to no row", async () => {
      await goLive();
      await pollOnce({ added: [metaMark("91", "_paid_via", "bacs")], removed: [metaMark("93", "_old_flag", "1")] });
      const list = screen.getByRole("region", { name: "Changes" });
      expect(within(list).getByRole("button", { name: /_paid_via/ })).toHaveTextContent("no row → bacs");
      expect(within(list).getByRole("button", { name: /_old_flag/ })).toHaveTextContent("1 → no row");
    });

    it("focuses the value of a line when it is clicked", async () => {
      await goLive();
      await pollOnce({ changed: [statusChange, totalChange] });
      await userEvent.click(within(screen.getByRole("region", { name: "Changes" })).getByRole("button", { name: /total_amount/ }));
      expect(activeObject().focus).toEqual(totalSource);
    });
  });

  describe("snapshots", () => {
    it("asks for a label on s and takes the snapshot of the row", async () => {
      await focusStatus();
      await userEvent.keyboard("s");
      const label = await screen.findByRole("textbox", { name: "Snapshot of wp_wc_orders #4711" });
      expect((label as HTMLInputElement).value).toMatch(/^snapshot \d\d:\d\d:\d\d$/);
      await userEvent.clear(label);
      await userEvent.type(label, "before deploy{Enter}");
      expect(mocked.snapshotTake).toHaveBeenCalledWith("s1", ORDER_URL, "before deploy");
      expect(await screen.findByText("Snapshot before deploy taken")).toBeInTheDocument();
    });

    it("says so when the row has no snapshots", async () => {
      await focusStatus();
      act(() => void runCommand("object.sinceSnapshot"));
      expect(await screen.findByText("No snapshots of wp_wc_orders #4711 yet")).toBeInTheDocument();
    });

    it("lists the snapshots and draws the marks since the picked one", async () => {
      mocked.snapshotList.mockResolvedValue([snapshot(), snapshot({ id: "snap-2", label: "older" })]);
      mocked.snapshotDiff.mockResolvedValue({ snapshot: snapshot(), delta: delta({ changed: [statusChange] }), document: null });
      await focusStatus();
      act(() => void runCommand("object.sinceSnapshot"));
      expect(await screen.findByRole("heading", { name: "Changes of wp_wc_orders #4711 since" })).toBeInTheDocument();
      expect(mocked.snapshotList).toHaveBeenCalledWith("s1", "db", ORDER_URL);
      expect(screen.getByRole("button", { name: /^older/ })).toBeInTheDocument();

      await userEvent.click(screen.getByRole("button", { name: /^before deploy/ }));
      expect(mocked.snapshotDiff).toHaveBeenCalledWith("s1", "snap-1");
      const list = await screen.findByRole("region", { name: "Changes" });
      expect(within(list).getByText(/since snapshot before deploy \(2 h ago\)/)).toBeInTheDocument();
      expect(rowOf("wc-processing")).toHaveClass("chg");
      expect(within(rowOf("wc-processing")).getByText("wc-pending")).toHaveClass("old");
    });

    it("says nothing changed when the picked snapshot equals the row", async () => {
      mocked.snapshotList.mockResolvedValue([snapshot()]);
      mocked.snapshotDiff.mockResolvedValue({ snapshot: snapshot(), delta: delta({}), document: null });
      await focusStatus();
      act(() => void runCommand("object.sinceSnapshot"));
      await userEvent.click(await screen.findByRole("button", { name: /^before deploy/ }));
      expect(await screen.findByText("Nothing changed since snapshot before deploy")).toBeInTheDocument();
      expect(screen.queryByRole("region", { name: "Changes" })).not.toBeInTheDocument();
    });

    it("deletes a snapshot from the dialog", async () => {
      mocked.snapshotList.mockResolvedValue([snapshot()]);
      await focusStatus();
      act(() => void runCommand("object.sinceSnapshot"));
      await userEvent.click(await screen.findByRole("button", { name: "Delete snapshot before deploy" }));
      expect(mocked.snapshotDelete).toHaveBeenCalledWith("snap-1");
      expect(await screen.findByText("No snapshots left.")).toBeInTheDocument();
    });
  });

  describe("compare", () => {
    const rightAddress: Address = { ...orderAddress, target: { kind: "row", table: "wp_wc_orders", key: "4712" } };
    const rightStatus = source("wp_wc_orders", "status", "4712");
    const rightOwn = {
      ...orderGroups.own,
      rows: [
        {
          key: [{ column: "id", value: "4712" }],
          fields: [
            ["status", value({ raw: text("wc-completed"), source: rightStatus })],
            ["total_amount", value({ raw: num("119.00"), kind: { kind: "number" as const }, source: source("wp_wc_orders", "total_amount", "4712") })],
          ] as [string, ReturnType<typeof value>][],
        },
      ],
    };

    beforeEach(() => {
      mocked.compare.mockResolvedValue({
        left: objectDocument({ groups: [orderGroups.own] }),
        right: objectDocument({ address: rightAddress, groups: [rightOwn] }),
        delta: delta({ changed: [{ group: "own", label: "status", source: rightStatus, oldSource: statusSource, old: text("wc-processing"), new: text("wc-completed") }] }),
      });
    });

    it("renders both rows and marks the differing value on both sides", async () => {
      await openOrder({ ...orderAddress, compare: "4712" });
      expect(await screen.findByText(/^1 difference · groups by rule/)).toBeInTheDocument();
      expect(mocked.compare).toHaveBeenCalledWith("s1", `${ORDER_URL}?compare=4712`);
      expect(rowOf("wc-processing")).toHaveClass("chg");
      expect(rowOf("wc-completed")).toHaveClass("chg");
      for (const total of screen.getAllByText("119.00")) expect(total.closest(".row")).not.toHaveClass("chg");
    });

    it("marks without flash or old value", async () => {
      await openOrder({ ...orderAddress, compare: "4712" });
      await screen.findByText("wc-completed");
      expect(document.querySelector(".flash")).toBeNull();
      expect(document.querySelector(".old")).toBeNull();
    });

    it("opens the comparison from the command after asking for the other key", async () => {
      await focusStatus();
      act(() => void runCommand("object.compare"));
      const key = await screen.findByRole("textbox", { name: "Compare wp_wc_orders #4711 with key" });
      await userEvent.type(key, "4712{Enter}");
      await waitFor(() => expect(formatAddress(activeObject().address)).toBe(`${ORDER_URL}?compare=4712`));
    });
  });
});
