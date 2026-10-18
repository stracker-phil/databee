import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useRef } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api, EMPTY_UI_STATE, type AnomalyLine, type CreatedEntity, type CreatedItem, type PinnedLine, type RecentLine, type Summary } from "../../api";
import { address, formatAddress } from "../../lib/address";
import { NO_HOVER, selectActiveTab, useStore } from "../../state/store";
import { connectedStore, readoutOf, readoutTab, text, value } from "../../test/fixtures";
import { activeObject } from "../../test/objectFixtures";
import { showTableIcons } from "../../test/iconFixtures";
import { session } from "../../test/session";
import { useValueHover } from "../value/useValueHover";
import { Readout } from "./Readout";

const mocked = vi.mocked(api, true);

const tab = readoutTab();
const posts = address("shop", "db", { kind: "table", table: "wp_posts", filter: [] });
const postRow = (key: string) => address("shop", "db", { kind: "row", table: "wp_posts", key });

const summary = (...pairs: [string, string][]): Summary => ({ values: pairs.map(([name, v]) => ({ name, value: value({ raw: text(v) }) })) });

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

const recent = (key: string, patch: Partial<RecentLine> = {}): RecentLine => ({ address: postRow(key), at: Math.floor(Date.now() / 1000), summary: { values: [] }, error: null, ...patch });

const entityOf = (entity: string, patch: Partial<CreatedEntity> = {}): CreatedEntity => ({
  entity,
  preset: "wordpress",
  table: "wp_posts",
  sql: `SELECT ID, post_title FROM wp_posts WHERE post_type = '${entity}' ORDER BY ID DESC LIMIT 5`,
  countInWindow: 2,
  error: null,
  ...patch,
});

const createdItem = (entity: string, key: string, patch: Partial<CreatedItem> = {}): CreatedItem => ({
  entity,
  table: "wp_posts",
  address: postRow(key),
  created: value({ raw: text("2026-01-15 10:00:00") }),
  summary: summary(["post_title", `Title ${key}`]),
  ...patch,
});

const withCreated = (items: CreatedItem[], entities: CreatedEntity[] = [entityOf("post"), entityOf("page", { countInWindow: 1 })]) =>
  readoutOf({ recentlyCreated: { items, entities, error: null } });

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

  it("shows the database, one meta line with host, prefix, presets and server version, and the status values", async () => {
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
    expect(status.getByRole("heading", { name: "db" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Status" })).toHaveTextContent("127.0.0.1 · wp_ · wordpress, woocommerce · 10.3.39-MariaDB");
    expect(status.getByRole("button", { name: "siteurl" })).toBeInTheDocument();
    expect(status.getByText("https://shop.test")).toBeInTheDocument();
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
    it.each(["Needs attention", "Recently created", "Recently opened"])("shows no %s section", async (name) => {
      await shown();
      expect(screen.queryByRole("region", { name })).not.toBeInTheDocument();
    });

    it("still shows the pinned section with its hint", async () => {
      await shown();
      expect(screen.getByRole("region", { name: "Pinned queries" })).toHaveTextContent("p pins a query in any SQL tab");
    });
  });

  describe("anomalies", () => {
    it("shows the count, text, preset and id of a line without coloring the count", async () => {
      await shown(readoutOf({ anomalies: { items: [anomaly()], error: null } }));
      const section = within(screen.getByRole("region", { name: "Needs attention" }));
      expect(section.getByText("auto-drafts older than a day")).toBeInTheDocument();
      expect(section.getByText("wordpress · auto-drafts")).toBeInTheDocument();
      expect(section.getByText("12")).not.toHaveAttribute("style");
    });

    it("opens the rows of the table with the line's filter", async () => {
      await shown(readoutOf({ anomalies: { items: [anomaly()], error: null } }));
      await userEvent.click(screen.getByRole("button", { name: "Show rows" }));
      expect(selectActiveTab(useStore.getState())).toMatchObject({ kind: "table", table: "wp_posts", filter: "post_status = 'auto-draft'" });
    });

    it("opens a saved query address as a query tab", async () => {
      const query = { id: "orphans", title: "Orphaned meta", sql: "SELECT 1", pin: null, origin: { kind: "project" } as const, readOnly: false };
      mocked.savedQueries.mockResolvedValue([query]);
      await shown(readoutOf({ anomalies: { items: [anomaly({ address: address("shop", "db", { kind: "query", id: "orphans" }), filter: null })], error: null } }));
      await userEvent.click(screen.getByRole("button", { name: "Show rows" }));
      await vi.waitFor(() => expect(selectActiveTab(useStore.getState())).toMatchObject({ kind: "query", title: "Orphaned meta" }));
    });

    it("opens the fix action's sheet on its table instead of queueing it", async () => {
      const fix = { name: "delete-auto-drafts", preset: "wordpress", kind: "delete" as const, undo: true, params: [], writes: [{ write: "deleteRows" as const, table: "wp_posts" }] };
      mocked.tableActions.mockResolvedValue([fix]);
      await shown(readoutOf({ anomalies: { items: [anomaly({ fix: { action: "delete-auto-drafts", address: posts } })], error: null } }));
      await userEvent.click(screen.getByRole("button", { name: "delete-auto-drafts" }));
      expect(mocked.tableActions).toHaveBeenCalledWith("s1", "db", "wp_posts");
      await vi.waitFor(() => expect(useStore.getState().dialog).toEqual({ type: "action", address: posts, action: fix }));
      expect(useStore.getState().pending).toHaveLength(0);
    });

    it("offers no Show rows button for a line without an address", async () => {
      await shown(readoutOf({ anomalies: { items: [anomaly({ address: null, filter: null })], error: null } }));
      expect(screen.queryByRole("button", { name: "Show rows" })).not.toBeInTheDocument();
    });

    it("shows a failing rule's error next to its line", async () => {
      await shown(readoutOf({ anomalies: { items: [anomaly({ count: null, error: "Unknown column 'x'" })], error: null } }));
      expect(screen.getByRole("region", { name: "Needs attention" })).toHaveTextContent("Unknown column 'x'");
    });
  });

  describe("part errors", () => {
    it("shows an error of a part in its own section", async () => {
      await shown(readoutOf({ anomalies: { items: [], error: "preset wordpress: rule broken" } }));
      expect(within(screen.getByRole("region", { name: "Needs attention" })).getByText("preset wordpress: rule broken")).toBeInTheDocument();
    });

    it("shows an error of the recently created part in its section", async () => {
      await shown(readoutOf({ recentlyCreated: { items: [], entities: [], error: "boom" } }));
      expect(screen.getByRole("region", { name: "Recently created" })).toHaveTextContent("boom");
    });

    it("keeps the other sections when one part fails", async () => {
      await shown(readoutOf({ recentlyCreated: { items: [], entities: [], error: "boom" }, pinned: { items: [pinned(3)], error: null } }));
      expect(screen.getByRole("region", { name: "Recently created" })).toHaveTextContent("boom");
      expect(screen.getByRole("button", { name: "Open drafts" })).toBeInTheDocument();
    });

    it("shows the error when the readout cannot be loaded", async () => {
      mocked.readout.mockRejectedValue({ kind: "io", message: "connection lost", code: null });
      render(<Readout tab={tab} />);
      expect(await screen.findByText("connection lost")).toBeInTheDocument();
    });
  });

  describe("recently created", () => {
    const section = () => within(screen.getByRole("region", { name: "Recently created" }));

    it("lists each row with its entity, key button and summary names and values", async () => {
      await shown(withCreated([createdItem("post", "7", { summary: summary(["post_title", "Hello"], ["post_status", "publish"]) })]));
      const line = section().getByRole("button", { name: "#7" }).closest<HTMLElement>(".row")!;
      expect(line).toHaveTextContent("post");
      for (const part of ["post_title", "Hello", "post_status", "publish"]) expect(within(line).getByText(part)).toBeInTheDocument();
    });

    it("opens the row of a line by its key", async () => {
      await shown(withCreated([createdItem("post", "7")]));
      await userEvent.click(section().getByRole("button", { name: "#7" }));
      expect(formatAddress(activeObject().address)).toBe("databee://shop/db/wp_posts/7");
    });

    it("shows the relative time of a timestamp with the stored value in its title", async () => {
      const utc = Date.now() / 1000 - 2 * 3600;
      const created = value({ raw: { t: "temporal", v: "2026-01-15 10:00:00" }, kind: { kind: "timestamp", utc, zone: null } });
      await shown(withCreated([createdItem("post", "7", { created })]));
      const time = section().getByText("2 h ago");
      expect(time.getAttribute("title")).toContain("2026-01-15 10:00:00");
    });

    it("shows the stored value when it is no timestamp", async () => {
      await shown(withCreated([createdItem("post", "7")]));
      expect(section().getByText("2026-01-15 10:00:00")).toBeInTheDocument();
    });

    it("shows a chip per entity with its count in the window, all pressed", async () => {
      await shown(withCreated([createdItem("post", "7")]));
      const chips = section().getByRole("group", { name: "Entities" });
      expect(within(chips).getByRole("button", { name: "post 2" })).toHaveAttribute("aria-pressed", "true");
      expect(within(chips).getByRole("button", { name: "page 1" })).toHaveAttribute("aria-pressed", "true");
    });

    it("hides the lines of an entity when its chip is toggled off and shows them again when toggled on", async () => {
      await shown(withCreated([createdItem("post", "7", { summary: summary(["post_title", "Hello"]) }), createdItem("page", "3", { summary: summary(["post_title", "About"]) })]));
      const chip = section().getByRole("button", { name: "post 2" });
      await userEvent.click(chip);
      expect(chip).toHaveAttribute("aria-pressed", "false");
      expect(section().queryByText("Hello")).not.toBeInTheDocument();
      expect(section().getByText("About")).toBeInTheDocument();
      await userEvent.click(chip);
      expect(chip).toHaveAttribute("aria-pressed", "true");
      expect(section().getByText("Hello")).toBeInTheDocument();
    });

    it("discloses the query and table of an entity from its chevron", async () => {
      const post = entityOf("post");
      await shown(withCreated([createdItem("post", "7")], [post]));
      expect(section().queryByText(post.sql)).not.toBeInTheDocument();
      await userEvent.click(section().getByRole("button", { name: "Query of post" }));
      expect(section().getByText(post.sql)).toBeInTheDocument();
      expect(section().getByRole("button", { name: "wp_posts" })).toBeInTheDocument();
      expect(section().getByRole("button", { name: "Query of post" })).toHaveAttribute("aria-expanded", "true");
    });

    it("marks an entity whose query failed", async () => {
      await shown(withCreated([], [entityOf("post", { error: "Unknown column 'x'" })]));
      expect(section().getByRole("button", { name: "post failed" })).toBeInTheDocument();
    });

    it("says no rows when every entity has none", async () => {
      await shown(withCreated([], [entityOf("post", { countInWindow: 0 })]));
      expect(section().getByText("no rows")).toBeInTheDocument();
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
    it("shows the table a value is stored in after its value", async () => {
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
    const full = () =>
      readoutOf({
        anomalies: { items: [anomaly()], error: null },
        recentlyCreated: { items: [createdItem("post", "7")], entities: [entityOf("post")], error: null },
        pinned: { items: [pinned(3)], error: null },
        recent: { items: [recent("7")], error: null },
      });

    it.each([
      ["Needs attention", "1 rule"],
      ["Recently created", "1 row"],
      ["Pinned queries", "1 query"],
      ["Recently opened", "1 object"],
    ])("shows the count of the %s band", async (name, count) => {
      await shown(full());
      expect(within(screen.getByRole("region", { name })).getByText(count)).toBeInTheDocument();
    });

    it("pluralizes the count of a band", async () => {
      await shown(readoutOf({ anomalies: { items: [anomaly(), anomaly({ id: "orphans" })], error: null }, recentlyCreated: { items: [createdItem("post", "7"), createdItem("post", "8")], entities: [entityOf("post")], error: null } }));
      expect(screen.getByText("2 rules")).toBeInTheDocument();
      expect(screen.getByText("2 rows")).toBeInTheDocument();
    });

    it("counts only the lines of the entities that are shown", async () => {
      await shown(withCreated([createdItem("post", "7"), createdItem("page", "3")]));
      await userEvent.click(screen.getByRole("button", { name: "page 1" }));
      expect(within(screen.getByRole("region", { name: "Recently created" })).getByText("1 row")).toBeInTheDocument();
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
      await userEvent.click(screen.getByRole("button", { name: "Collapse needs attention" }));
      const toggle = screen.getByRole("button", { name: "Expand needs attention" });
      expect(toggle).toHaveAttribute("aria-expanded", "false");
      expect(screen.queryByText(anomalyText)).not.toBeInTheDocument();
      expect(within(screen.getByRole("region", { name: "Needs attention" })).getByText("1 rule")).toBeInTheDocument();
    });

    it("shows the rows again when expanded", async () => {
      await shown(withAnomaly());
      await userEvent.click(screen.getByRole("button", { name: "Collapse needs attention" }));
      await userEvent.click(screen.getByRole("button", { name: "Expand needs attention" }));
      expect(screen.getByText(anomalyText)).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Collapse needs attention" })).toHaveAttribute("aria-expanded", "true");
    });

    it("toggles when the band is clicked outside its buttons", async () => {
      await shown(withAnomaly());
      await userEvent.click(within(screen.getByRole("region", { name: "Needs attention" })).getByText("needs attention"));
      expect(screen.queryByText(anomalyText)).not.toBeInTheDocument();
      await userEvent.click(within(screen.getByRole("region", { name: "Needs attention" })).getByText("1 rule"));
      expect(screen.getByText(anomalyText)).toBeInTheDocument();
    });

    it("remembers the collapsed band per database and keeps it collapsed after a reload", async () => {
      await shown(withAnomaly());
      await userEvent.click(screen.getByRole("button", { name: "Collapse needs attention" }));
      expect(useStore.getState().uiState[session.identity]?.collapsedGroups.db).toEqual(["::anomalies"]);

      mocked.readout.mockResolvedValue(readoutOf({ anomalies: { items: [anomaly(), anomaly({ id: "orphans" })], error: null } }));
      act(() => useStore.getState().bumpData());
      await screen.findByText("2 rules");
      expect(screen.getByRole("button", { name: "Expand needs attention" })).toBeInTheDocument();
      expect(screen.queryByText(anomalyText)).not.toBeInTheDocument();
    });

    it("starts collapsed when the band was collapsed before", async () => {
      useStore.setState({ uiState: { [session.identity]: { ...EMPTY_UI_STATE, collapsedGroups: { db: ["::pinned"] } } } });
      await shown(readoutOf({ pinned: { items: [pinned(3)], error: null } }));
      expect(screen.getByRole("button", { name: "Expand pinned" })).toHaveAttribute("aria-expanded", "false");
      expect(screen.queryByRole("button", { name: "Open drafts" })).not.toBeInTheDocument();
    });

    it.each([
      ["created", "recently created"],
      ["recent", "recently opened"],
    ])("remembers the %s band under its own key", async (key, name) => {
      await shown(readoutOf({ recentlyCreated: { items: [createdItem("post", "7")], entities: [entityOf("post")], error: null }, recent: { items: [recent("7")], error: null } }));
      await userEvent.click(screen.getByRole("button", { name: `Collapse ${name}` }));
      expect(useStore.getState().uiState[session.identity]?.collapsedGroups.db).toEqual([`::${key}`]);
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

    it("names the table of an anomaly card when its empty space is hovered", async () => {
      await shownHovering(readoutOf({ anomalies: { items: [anomaly()], error: null } }));
      await hover(screen.getByText("12"));
      expect(useStore.getState().hover).toEqual({ src: null, table: "wp_posts", linked: false });
    });

    it("names the table of a recently created line", async () => {
      await shownHovering(withCreated([createdItem("post", "7")]));
      await hover(screen.getByRole("button", { name: "#7" }).closest<HTMLElement>(".row")!);
      expect(useStore.getState().hover).toEqual({ src: null, table: "wp_posts", linked: false });
    });

    it("names the table of a recent object row", async () => {
      await shownHovering(readoutOf({ recent: { items: [recent("7")], error: null } }));
      await hover(within(screen.getByRole("region", { name: "Recently opened" })).getByText("wp_posts"));
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

  describe("table icons and hints", () => {
    const hint = (el: HTMLElement) => el.querySelector("[data-open]")?.getAttribute("data-open") ?? null;
    const icon = (el: Element) => el.querySelector("[data-icon]")?.getAttribute("data-icon") ?? null;

    beforeEach(() => showTableIcons({ wp_posts: "file-text" }));

    it("draws the icon of the table in a recently created line and hints that its key goes to the row", async () => {
      await shown(withCreated([createdItem("post", "7")]));
      const key = within(screen.getByRole("region", { name: "Recently created" })).getByRole("button", { name: "#7" });
      expect(hint(key)).toBe("go");
      expect(icon(key.closest(".row")!)).toBe("file-text");
    });

    it("draws the icon of the table in a recently opened line and hints that its key goes to the row", async () => {
      await shown(readoutOf({ recent: { items: [recent("7")], error: null } }));
      const key = within(screen.getByRole("region", { name: "Recently opened" })).getByRole("button", { name: "#7" });
      expect(hint(key)).toBe("go");
      expect(icon(key.closest(".row")!)).toBe("file-text");
    });

    it("draws no icon for a table without one", async () => {
      showTableIcons({});
      await shown(readoutOf({ recent: { items: [recent("7")], error: null } }));
      expect(icon(screen.getByRole("button", { name: "#7" }).closest(".row")!)).toBeNull();
    });

    it("draws the icon and a hint on the table button of an entity's query", async () => {
      await shown(withCreated([createdItem("post", "7")], [entityOf("post")]));
      await userEvent.click(screen.getByRole("button", { name: "Query of post" }));
      const table = screen.getByRole("button", { name: "wp_posts" });
      expect(icon(table)).toBe("file-text");
      expect(hint(table)).toBe("go");
    });

    it("draws the icon of the table a status value is stored in and hints that its title goes to the query", async () => {
      const base = readoutOf();
      const source = { table: "wp_posts", column: "post_title", row: [{ column: "ID", value: "1" }] };
      await shown({
        ...base,
        status: {
          ...base.status,
          values: { items: [{ id: "front", title: "front", sql: "SELECT 1", address: address("shop", "db", { kind: "query", id: "front" }), value: value({ raw: text("Home"), source }), error: null }], error: null },
        },
      });
      const status = screen.getByRole("region", { name: "Status" });
      expect(hint(within(status).getByRole("button", { name: "front" }))).toBe("go");
      expect(icon(within(status).getByText("wp_posts"))).toBe("file-text");
    });

    it("hints that Show rows and a pinned title go", async () => {
      await shown(readoutOf({ anomalies: { items: [anomaly()], error: null }, pinned: { items: [pinned(3)], error: null } }));
      expect(hint(screen.getByRole("button", { name: "Show rows" }))).toBe("go");
      expect(hint(screen.getByRole("button", { name: "Open drafts" }))).toBe("go");
    });
  });

  describe("recently opened", () => {
    it("opens the row of a recent object by its key", async () => {
      await shown(readoutOf({ recent: { items: [recent("7")], error: null } }));
      expect(within(screen.getByRole("region", { name: "Recently opened" })).getByText("wp_posts")).toBeInTheDocument();
      await userEvent.click(screen.getByRole("button", { name: "#7" }));
      expect(formatAddress(activeObject().address)).toBe("databee://shop/db/wp_posts/7");
      expect(mocked.rememberObject).toHaveBeenCalledWith("s1", "databee://shop/db/wp_posts/7");
    });

    it("renders the summary of a recent object as names and values", async () => {
      await shown(readoutOf({ recent: { items: [recent("7", { summary: summary(["post_title", "Hello"]) })], error: null } }));
      const section = within(screen.getByRole("region", { name: "Recently opened" }));
      expect(section.getByText("post_title")).toBeInTheDocument();
      expect(section.getByText("Hello")).toBeInTheDocument();
    });

    it("shows the error of a recent object instead of its summary", async () => {
      await shown(readoutOf({ recent: { items: [recent("7", { error: "Row is gone" })], error: null } }));
      expect(within(screen.getByRole("region", { name: "Recently opened" })).getByText("Row is gone")).toBeInTheDocument();
    });
  });
});
