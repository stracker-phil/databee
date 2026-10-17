import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api, type AnomalyLine, type PinnedLine, type RecentLine } from "../../api";
import { address, formatAddress } from "../../lib/address";
import { selectActiveTab, useStore } from "../../state/store";
import { connectedStore, readoutOf, readoutTab, text, value } from "../../test/fixtures";
import { activeObject } from "../../test/objectFixtures";
import { Readout } from "./Readout";

const mocked = vi.mocked(api, true);

const tab = readoutTab();
const posts = address("shop", "db", { kind: "table", table: "wp_posts", filter: [] });

const anomaly = (patch: Partial<AnomalyLine> = {}): AnomalyLine => ({
  id: "auto-drafts",
  preset: "wordpress",
  text: "auto-drafts older than a day",
  count: "12",
  address: posts,
  filter: "post_status = 'auto-draft'",
  fix: null,
  error: null,
  ...patch,
});

const pinned = (count: number): PinnedLine => ({
  id: "drafts",
  title: "Open drafts",
  sql: "SELECT COUNT(*) FROM wp_posts",
  origin: { kind: "project" },
  address: address("shop", "db", { kind: "query", id: "drafts" }),
  result: { show: "count", count },
  error: null,
});

const recent = (key: string): RecentLine => ({ address: address("shop", "db", { kind: "row", table: "wp_posts", key }), at: Math.floor(Date.now() / 1000), summary: [], error: null });

const shown = async (readout = readoutOf()) => {
  mocked.readout.mockResolvedValue(readout);
  render(<Readout tab={tab} />);
  await screen.findByRole("region", { name: "Status" });
};

describe("Readout", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocked.savedQueries.mockResolvedValue([]);
    connectedStore({ tabs: [tab] });
  });

  it("shows the raw status line with host, database, prefix, presets, server version and status values", async () => {
    const base = readoutOf();
    await shown({
      ...base,
      status: {
        ...base.status,
        prefix: "wp_",
        presets: ["wordpress", "woocommerce"],
        values: {
          items: [{ id: "siteurl", title: "siteurl", sql: "SELECT 1", address: address("shop", "db", { kind: "query", id: "siteurl" }), value: value({ raw: text("https://shop.test") }), error: null }],
          error: null,
        },
      },
    });
    const status = within(screen.getByRole("region", { name: "Status" }));
    for (const part of ["127.0.0.1", "db", "wp_", "wordpress, woocommerce", "10.3.39-MariaDB", "siteurl", "https://shop.test"]) {
      expect(status.getByText(part)).toBeInTheDocument();
    }
  });

  it("opens the saved query of a status value", async () => {
    const query = { id: "siteurl", title: "siteurl", sql: "SELECT option_value FROM wp_options", pin: null, origin: { kind: "project" } as const, readOnly: false };
    mocked.savedQueries.mockResolvedValue([query]);
    const base = readoutOf();
    await shown({
      ...base,
      status: { ...base.status, values: { items: [{ id: "siteurl", title: "siteurl", sql: query.sql, address: address("shop", "db", { kind: "query", id: "siteurl" }), value: null, error: null }], error: null } },
    });
    expect(screen.getByText("no row")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "siteurl" }));
    await vi.waitFor(() => expect(mocked.execute).toHaveBeenCalledWith("s1", query.sql, false));
    expect(selectActiveTab(useStore.getState())).toMatchObject({ kind: "query", title: "siteurl" });
  });

  describe("without presets", () => {
    it.each(["Anomalies", "New objects", "Recent"])("shows no %s section", async (name) => {
      await shown();
      expect(screen.queryByRole("region", { name })).not.toBeInTheDocument();
    });

    it("still shows the pinned section with its hint", async () => {
      await shown();
      expect(screen.getByRole("region", { name: "Pinned queries" })).toHaveTextContent("p pins a query in any SQL tab");
    });
  });

  describe("anomalies", () => {
    it("shows the count and text of a line without coloring the count", async () => {
      await shown(readoutOf({ anomalies: { items: [anomaly()], error: null } }));
      const section = within(screen.getByRole("region", { name: "Anomalies" }));
      expect(section.getByText("auto-drafts older than a day")).toBeInTheDocument();
      expect(section.getByText("12")).not.toHaveAttribute("style");
    });

    it("opens the rows of the table with the line's filter", async () => {
      await shown(readoutOf({ anomalies: { items: [anomaly()], error: null } }));
      await userEvent.click(screen.getByRole("button", { name: "→ rows" }));
      expect(selectActiveTab(useStore.getState())).toMatchObject({ kind: "table", table: "wp_posts", filter: "post_status = 'auto-draft'" });
    });

    it("opens a saved query address as a query tab", async () => {
      const query = { id: "orphans", title: "Orphaned meta", sql: "SELECT 1", pin: null, origin: { kind: "project" } as const, readOnly: false };
      mocked.savedQueries.mockResolvedValue([query]);
      await shown(readoutOf({ anomalies: { items: [anomaly({ address: address("shop", "db", { kind: "query", id: "orphans" }), filter: null })], error: null } }));
      await userEvent.click(screen.getByRole("button", { name: "→ rows" }));
      await vi.waitFor(() => expect(selectActiveTab(useStore.getState())).toMatchObject({ kind: "query", title: "Orphaned meta" }));
    });

    it("queues the fix action for review", async () => {
      await shown(readoutOf({ anomalies: { items: [anomaly({ fix: { action: "delete-auto-drafts", address: posts } })], error: null } }));
      await userEvent.click(screen.getByRole("button", { name: "delete-auto-drafts" }));
      expect(mocked.planAction).toHaveBeenCalledWith("s1", formatAddress(posts), "delete-auto-drafts", {});
      await vi.waitFor(() => expect(useStore.getState().pending).toHaveLength(1));
    });

    it("offers no rows link for a line without an address", async () => {
      await shown(readoutOf({ anomalies: { items: [anomaly({ address: null, filter: null })], error: null } }));
      expect(screen.queryByRole("button", { name: "→ rows" })).not.toBeInTheDocument();
    });

    it("shows a failing rule's error next to its line", async () => {
      await shown(readoutOf({ anomalies: { items: [anomaly({ count: null, error: "Unknown column 'x'" })], error: null } }));
      expect(screen.getByRole("region", { name: "Anomalies" })).toHaveTextContent("Unknown column 'x'");
    });
  });

  describe("part errors", () => {
    it("shows an error of a part in its own section", async () => {
      await shown(readoutOf({ anomalies: { items: [], error: "preset wordpress: rule broken" } }));
      expect(within(screen.getByRole("region", { name: "Anomalies" })).getByText("preset wordpress: rule broken")).toBeInTheDocument();
    });

    it("keeps the other sections when one part fails", async () => {
      await shown(readoutOf({ newObjects: { items: [], error: "boom" }, pinned: { items: [pinned(3)], error: null } }));
      expect(screen.getByRole("region", { name: "New objects" })).toHaveTextContent("boom");
      expect(screen.getByRole("button", { name: "Open drafts" })).toBeInTheDocument();
    });

    it("shows the error when the readout cannot be loaded", async () => {
      mocked.readout.mockRejectedValue({ kind: "io", message: "connection lost", code: null });
      render(<Readout tab={tab} />);
      expect(await screen.findByText("connection lost")).toBeInTheDocument();
    });
  });

  describe("new objects", () => {
    it("lists the newest rows of an entity with the SQL behind them", async () => {
      await shown(
        readoutOf({
          newObjects: {
            items: [{ entity: "post", preset: "wordpress", table: "wp_posts", sql: "SELECT ID, post_title FROM wp_posts ORDER BY ID DESC LIMIT 5", columns: ["ID", "post_title"], rows: [{ address: null, values: [value({ raw: text("7") }), value({ raw: text("Hello") })] }], error: null }],
            error: null,
          },
        }),
      );
      const section = within(screen.getByRole("region", { name: "New objects" }));
      expect(section.getByText("post_title")).toBeInTheDocument();
      expect(section.getByText("Hello")).toBeInTheDocument();
      expect(section.getByText(/SELECT ID, post_title FROM wp_posts/)).toBeInTheDocument();
    });
  });

  describe("pinned queries", () => {
    it("reloads the count after an apply", async () => {
      await shown(readoutOf({ pinned: { items: [pinned(3)], error: null } }));
      expect(await screen.findByText("3")).toBeInTheDocument();

      mocked.readout.mockResolvedValue(readoutOf({ pinned: { items: [pinned(2)], error: null } }));
      act(() => useStore.getState().bumpData());

      expect(await screen.findByText("2")).toBeInTheDocument();
      expect(screen.queryByText("3")).not.toBeInTheDocument();
    });
  });

  describe("recent objects", () => {
    it("opens the row of a recent object by its key", async () => {
      await shown(readoutOf({ recent: { items: [recent("7")], error: null } }));
      expect(within(screen.getByRole("region", { name: "Recent" })).getByText("wp_posts")).toBeInTheDocument();
      await userEvent.click(screen.getByRole("button", { name: "#7" }));
      expect(formatAddress(activeObject().address)).toBe("databee://shop/db/wp_posts/7");
      expect(mocked.rememberObject).toHaveBeenCalledWith("s1", "databee://shop/db/wp_posts/7");
    });
  });
});
