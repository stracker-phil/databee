import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useRef } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api, EMPTY_UI_STATE, type AnomalyLine, type NewObjects, type PinnedLine, type RecentLine } from "../../api";
import { address, formatAddress } from "../../lib/address";
import { NO_HOVER, selectActiveTab, useStore } from "../../state/store";
import { connectedStore, readoutOf, readoutTab, text, value } from "../../test/fixtures";
import { activeObject } from "../../test/objectFixtures";
import { session } from "../../test/session";
import { useValueHover } from "../value/useValueHover";
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

const newPosts = (): NewObjects => ({
  entity: "post",
  preset: "wordpress",
  table: "wp_posts",
  sql: "SELECT ID, post_title FROM wp_posts ORDER BY ID DESC LIMIT 5",
  columns: ["ID", "post_title"],
  rows: [{ address: null, values: [value({ raw: text("7") }), value({ raw: text("Hello") })] }],
  error: null,
});

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

  describe("status values", () => {
    it("names the table a value is stored in before its title", async () => {
      const base = readoutOf();
      const source = { table: "wp_options", column: "option_value", row: [{ column: "option_name", value: "siteurl" }] };
      await shown({
        ...base,
        status: {
          ...base.status,
          values: {
            items: [{ id: "siteurl", title: "siteurl", sql: "SELECT 1", address: address("shop", "db", { kind: "query", id: "siteurl" }), value: value({ raw: text("https://shop.test"), source }), error: null }],
            error: null,
          },
        },
      });
      const status = within(screen.getByRole("region", { name: "Status" }));
      expect(status.getByText("wp_options")).toBeInTheDocument();
      expect(status.getByRole("button", { name: "siteurl" })).toBeInTheDocument();
    });
  });

  describe("band counts and hints", () => {
    it.each([
      ["Anomalies", "1 rule"],
      ["New objects", "1 entity"],
      ["Pinned queries", "1 query"],
      ["Recent", "1 object"],
    ])("shows the count of the %s band", async (name, count) => {
      await shown(readoutOf({ anomalies: { items: [anomaly()], error: null }, newObjects: { items: [newPosts()], error: null }, pinned: { items: [pinned(3)], error: null }, recent: { items: [recent("7")], error: null } }));
      expect(within(screen.getByRole("region", { name })).getByText(count)).toBeInTheDocument();
    });

    it("pluralizes the count of a band", async () => {
      await shown(readoutOf({ anomalies: { items: [anomaly(), anomaly({ id: "orphans" })], error: null }, newObjects: { items: [newPosts(), { ...newPosts(), entity: "page" }], error: null } }));
      expect(screen.getByText("2 rules")).toBeInTheDocument();
      expect(screen.getByText("2 entities")).toBeInTheDocument();
    });

    it("shows how to pin and that nothing is pinned yet", async () => {
      await shown();
      const section = within(screen.getByRole("region", { name: "Pinned queries" }));
      expect(section.getByText("0 queries")).toBeInTheDocument();
      expect(section.getByText(/p pins a query in any SQL tab/)).toBeInTheDocument();
      expect(section.getByText("no pinned queries")).toBeInTheDocument();
    });
  });

  describe("collapsing a band", () => {
    const withAnomaly = () => readoutOf({ anomalies: { items: [anomaly()], error: null } });
    const anomalyText = "auto-drafts older than a day";

    it("hides the rows but keeps the band and the region", async () => {
      await shown(withAnomaly());
      await userEvent.click(screen.getByRole("button", { name: "Collapse anomalies" }));
      const toggle = screen.getByRole("button", { name: "Expand anomalies" });
      expect(toggle).toHaveAttribute("aria-expanded", "false");
      expect(screen.queryByText(anomalyText)).not.toBeInTheDocument();
      expect(within(screen.getByRole("region", { name: "Anomalies" })).getByText("1 rule")).toBeInTheDocument();
    });

    it("shows the rows again when expanded", async () => {
      await shown(withAnomaly());
      await userEvent.click(screen.getByRole("button", { name: "Collapse anomalies" }));
      await userEvent.click(screen.getByRole("button", { name: "Expand anomalies" }));
      expect(screen.getByText(anomalyText)).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Collapse anomalies" })).toHaveAttribute("aria-expanded", "true");
    });

    it("toggles when the band is clicked outside its buttons", async () => {
      await shown(withAnomaly());
      await userEvent.click(within(screen.getByRole("region", { name: "Anomalies" })).getByText("anomalies"));
      expect(screen.queryByText(anomalyText)).not.toBeInTheDocument();
      await userEvent.click(within(screen.getByRole("region", { name: "Anomalies" })).getByText("1 rule"));
      expect(screen.getByText(anomalyText)).toBeInTheDocument();
    });

    it("remembers the collapsed band per database and keeps it collapsed after a reload", async () => {
      await shown(withAnomaly());
      await userEvent.click(screen.getByRole("button", { name: "Collapse anomalies" }));
      expect(useStore.getState().uiState[session.identity]?.collapsedGroups.db).toEqual(["::anomalies"]);

      mocked.readout.mockResolvedValue(readoutOf({ anomalies: { items: [anomaly(), anomaly({ id: "orphans" })], error: null } }));
      act(() => useStore.getState().bumpData());
      await screen.findByText("2 rules");
      expect(screen.getByRole("button", { name: "Expand anomalies" })).toBeInTheDocument();
      expect(screen.queryByText(anomalyText)).not.toBeInTheDocument();
    });

    it("starts collapsed when the band was collapsed before", async () => {
      useStore.setState({ uiState: { [session.identity]: { ...EMPTY_UI_STATE, collapsedGroups: { db: ["::pinned"] } } } });
      await shown(readoutOf({ pinned: { items: [pinned(3)], error: null } }));
      expect(screen.getByRole("button", { name: "Expand pinned" })).toHaveAttribute("aria-expanded", "false");
      expect(screen.queryByRole("button", { name: "Open drafts" })).not.toBeInTheDocument();
    });
  });

  describe("new objects block", () => {
    it("shows the entity, its table and its row count, and opens the table", async () => {
      await shown(readoutOf({ newObjects: { items: [newPosts()], error: null } }));
      const section = within(screen.getByRole("region", { name: "New objects" }));
      expect(section.getByText("post")).toBeInTheDocument();
      expect(section.getByText("1 row")).toBeInTheDocument();
      await userEvent.click(section.getByRole("button", { name: "wp_posts" }));
      expect(selectActiveTab(useStore.getState())).toMatchObject({ kind: "table", table: "wp_posts" });
    });
  });

  describe("pinned rows query", () => {
    it("shows the title, the row count, the SQL and the rows", async () => {
      const line: PinnedLine = {
        ...pinned(0),
        id: "latest",
        title: "Latest posts",
        sql: "SELECT ID, post_title FROM wp_posts LIMIT 2",
        result: { show: "rows", columns: ["ID", "post_title"], rows: [{ address: null, values: [value({ raw: text("7") }), value({ raw: text("Hello") })] }, { address: null, values: [value({ raw: text("8") }), value({ raw: text("World") })] }] },
      };
      await shown(readoutOf({ pinned: { items: [line], error: null } }));
      const section = within(screen.getByRole("region", { name: "Pinned queries" }));
      expect(section.getByRole("button", { name: "Latest posts" })).toBeInTheDocument();
      expect(section.getByText("2 rows")).toBeInTheDocument();
      expect(section.getByText("SELECT ID, post_title FROM wp_posts LIMIT 2")).toBeInTheDocument();
      for (const cell of ["post_title", "Hello", "World"]) expect(section.getByText(cell)).toBeInTheDocument();
    });

    it("renders only a window of 500 rows", async () => {
      const rows = Array.from({ length: 500 }, (_, i) => ({ address: null, values: [value({ raw: text(`post_${String(i + 1).padStart(4, "0")}`) })] }));
      const line: PinnedLine = { ...pinned(0), title: "Many posts", result: { show: "rows", columns: ["post_title"], rows } };
      await shown(readoutOf({ pinned: { items: [line], error: null } }));
      const section = within(screen.getByRole("region", { name: "Pinned queries" }));
      expect(section.getByText("500 rows")).toBeInTheDocument();
      expect(section.getByText("post_0001")).toBeInTheDocument();
      expect(section.queryByText("post_0500")).not.toBeInTheDocument();
    });
  });

  describe("row hover", () => {
    function Hovering() {
      const ref = useRef<HTMLDivElement>(null);
      useValueHover(ref);
      return (
        <div ref={ref}>
          <Readout tab={tab} />
        </div>
      );
    }

    const shownHovering = async (readout = readoutOf()) => {
      mocked.readout.mockResolvedValue(readout);
      render(<Hovering />);
      await screen.findByRole("region", { name: "Status" });
    };
    const hover = async (el: HTMLElement) => {
      await userEvent.hover(el);
      await vi.waitFor(() => expect(useStore.getState().hover).not.toEqual(NO_HOVER));
    };

    beforeEach(() => useStore.setState({ hover: NO_HOVER }));

    it("names the table of an anomaly row when its empty space is hovered", async () => {
      await shownHovering(readoutOf({ anomalies: { items: [anomaly()], error: null } }));
      await hover(screen.getByText("12"));
      expect(useStore.getState().hover).toEqual({ src: null, table: "wp_posts", linked: false });
    });

    it("names the table of a row of a new-objects block", async () => {
      await shownHovering(readoutOf({ newObjects: { items: [newPosts()], error: null } }));
      await hover(screen.getByText("Hello").closest<HTMLElement>(".row")!);
      expect(useStore.getState().hover).toEqual({ src: null, table: "wp_posts", linked: false });
    });

    it("names the table of a recent object row", async () => {
      await shownHovering(readoutOf({ recent: { items: [recent("7")], error: null } }));
      await hover(within(screen.getByRole("region", { name: "Recent" })).getByText("wp_posts"));
      expect(useStore.getState().hover).toEqual({ src: null, table: "wp_posts", linked: false });
    });

    it("gives a hovered value its own provenance", async () => {
      const source = { table: "wp_options", column: "option_value", row: [{ column: "option_name", value: "siteurl" }] };
      const base = readoutOf();
      await shownHovering({
        ...base,
        status: {
          ...base.status,
          values: { items: [{ id: "siteurl", title: "siteurl", sql: "SELECT 1", address: address("shop", "db", { kind: "query", id: "siteurl" }), value: value({ raw: text("https://shop.test"), source }), error: null }], error: null },
        },
      });
      await hover(screen.getByText("https://shop.test"));
      expect(useStore.getState().hover).toMatchObject({ table: "wp_options", linked: false });
      expect(useStore.getState().hover.src).toContain("option_value");
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
