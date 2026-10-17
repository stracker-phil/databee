import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import App from "../../App";
import { api, type Dashboard } from "../../api";
import { installCommands } from "../../commands";
import { useStore } from "../../state/store";
import { paletteHit } from "../../test/paletteFixtures";
import { NOW, orderDocument, wpStore } from "../../test/wpFixtures";
import { CommandPalette } from "../CommandPalette";
import { Dialogs } from "../Dialogs";
import { Toasts } from "../Toasts";
import { PendingTray } from "../PendingTray";
import { SnapshotsScreen } from "./Debugging";
import { WpWorkspace } from "./WpWorkspace";

const mocked = vi.mocked(api, true);
const originalWidth = window.innerWidth;

/** `document` answers `<entity> <id>` with the order row of that id. */
const answerEntityDocuments = () => mocked.document.mockImplementation(async (_s, text) => orderDocument(text.split(" ")[1]));
const lastTab = () => useStore.getState().tabs.at(-1);

afterEach(() => {
  window.innerWidth = originalWidth;
  vi.useRealTimers();
});

const dashboard: Dashboard = {
  orders: { total: 974, today: 12, byStatus: [{ value: "wc-completed", label: "Completed", count: 900 }], days: [{ date: "2026-10-15", completed: 3, processing: 1, failed: 1, other: 0 }], granularity: "day" },
  autoloadBytes: 82_637,
  autoloadHealthy: true,
  transientsExpired: 11,
  actions: {
    total: 2611,
    byStatus: [{ value: "failed", label: "Failed", count: 3 }],
    overdue: 0,
  },
  hposDrift: 0,
  tables: [
    { name: "wp_woocommerce_log", rows: 44_000, bytes: 1, category: "logs" },
    { name: "wp_wc_orders_meta", rows: 9_500, bytes: 1, category: "woocommerce" },
  ],
  insights: [
    { level: "warning", text: "wp_woocommerce_log holds 54% of all rows. Consider a retention rule.", action: { label: "Set retention", change: { type: "deleteLogsBefore", database: "db", days: 30, source: null } } },
    { level: "danger", text: "3 scheduled actions failed.", action: null },
  ],
};

describe("WordPress mode", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    wpStore();
    mocked.wpDashboard.mockResolvedValue(dashboard);
    answerEntityDocuments();
  });

  it("opens WordPress databases on the dashboard", async () => {
    render(<App />);
    const ordersTile = within(await screen.findByRole("group", { name: "Orders" }));
    expect(ordersTile.getByText("974")).toBeInTheDocument();
    expect(ordersTile.getByText("+12 today")).toBeInTheDocument();
    expect(screen.getByText("80.7 KB")).toBeInTheDocument();
    expect(screen.getByText(/holds 54% of all rows/)).toBeInTheDocument();
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

  it("turns alerts into actions: changes queue with SQL", async () => {
    render(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "Set retention" }));
    expect(mocked.wpPlanChange).toHaveBeenCalledWith("s1", { type: "deleteLogsBefore", database: "db", days: 30, source: null });
    expect(within(await screen.findByRole("region", { name: "Pending changes" })).getByText("1 pending change")).toBeInTheDocument();
  });

  it("switches to the database view and back", async () => {
    render(<App />);
    await userEvent.click(screen.getByRole("button", { name: /Database/ }));
    expect(screen.getByRole("tree", { name: "Databases" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /WordPress/ }));
    expect(await screen.findByRole("group", { name: "Orders" })).toBeInTheDocument();
  });

  it("opens an entity as an object tab on the row Rust resolved it to", async () => {
    render(<App />);
    await useStore.getState().openEntity("order", "4711");
    const tab = await screen.findByRole("tab", { name: /order #4711/ });
    expect(tab).toHaveAttribute("title", "databee://shop/db/wp_wc_orders/4711");
    expect(mocked.document).toHaveBeenCalledWith("s1", "order 4711", "db");
    expect(useStore.getState().mode).toBe("db");
  });
});

describe("Command palette", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    wpStore();
    installCommands();
    answerEntityDocuments();
  });

  it("finds pasted identifiers and opens the match as an object tab", async () => {
    mocked.paletteSearch.mockResolvedValue([paletteHit("wp_wc_orders_meta.meta_value", "3229", "objects with value 3UT60541JW9211534", "wp_wc_orders")]);
    useStore.getState().setPaletteOpen(true);
    render(<CommandPalette />);
    await userEvent.type(screen.getByLabelText("Search everything"), "3UT60541JW9211534");
    expect(await screen.findByRole("option", { name: "wp_wc_orders_meta.meta_value #3229" })).toBeInTheDocument();
    expect(screen.getByText("objects with value 3UT60541JW9211534")).toBeInTheDocument();
    await userEvent.keyboard("{Enter}");
    await vi.waitFor(() => expect(lastTab()).toMatchObject({ kind: "object", address: { target: { kind: "row", table: "wp_wc_orders", key: "3229" } } }));
    expect(useStore.getState().paletteOpen).toBe(false);
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

  it("lists every table that has a row with a numeric ID", async () => {
    mocked.paletteSearch.mockResolvedValue([
      paletteHit("wp_wc_orders", "3229", "objects with id 3229"),
      paletteHit("wp_posts", "3229", "objects with id 3229"),
      paletteHit("wp_users", "3229", "objects with id 3229"),
    ]);
    useStore.getState().setPaletteOpen(true);
    render(<CommandPalette />);
    await userEvent.type(screen.getByLabelText("Search everything"), "#3229");
    expect(await screen.findByRole("option", { name: "wp_wc_orders #3229" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "wp_posts #3229" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "wp_users #3229" })).toBeInTheDocument();
    expect(mocked.paletteSearch).toHaveBeenCalledWith("s1", "db", "#3229");
  });
});

describe("Pending changes", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    wpStore();
  });

  it("plans WordPress changes through wpPlanChange, applies them by id in one batch and undoes that batch", async () => {
    mocked.wpPlanChange
      .mockResolvedValueOnce({ id: 11, description: "planned deleteExpiredTransients", statements: ["-- deleteExpiredTransients"], canUndo: true })
      .mockResolvedValueOnce({ id: 12, description: "planned setAutoload", statements: ["-- setAutoload"], canUndo: true });
    mocked.applyChanges.mockResolvedValueOnce({ batchId: 7, affected: 2, canUndo: true });
    await useStore.getState().queueChange({ type: "deleteExpiredTransients", database: "db" });
    await useStore.getState().queueChange({ type: "setAutoload", database: "db", option: "x", on: false });
    render(<PendingTray />);
    expect(screen.getByText("2 pending changes")).toBeInTheDocument();
    expect(mocked.planChange).not.toHaveBeenCalled();
    await userEvent.click(screen.getAllByRole("button", { name: "Show SQL" })[0]);
    expect(screen.getByText("-- deleteExpiredTransients")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Apply/ }));
    expect(mocked.applyChanges).toHaveBeenCalledWith("s1", [11, 12]);
    await userEvent.click(await screen.findByRole("button", { name: /Undo last apply/ }));
    expect(mocked.undoBatch).toHaveBeenCalledWith("s1", 7);
  });

  it("discards a single change on the server and removes it from the tray", async () => {
    mocked.wpPlanChange.mockResolvedValueOnce({ id: 21, description: "planned deleteExpiredTransients", statements: ["-- deleteExpiredTransients"], canUndo: true });
    await useStore.getState().queueChange({ type: "deleteExpiredTransients", database: "db" });
    render(<PendingTray />);
    await userEvent.click(screen.getByRole("button", { name: /Discard planned deleteExpiredTransients/ }));
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
});

