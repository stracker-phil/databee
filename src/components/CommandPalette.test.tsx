import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api, type PaletteHit, type RulesInfo, type SavedEntry } from "../api";
import { installCommands, registerCommands } from "../commands";
import { formatAddress } from "../lib/address";
import { projectKey } from "../state/slices/project";
import { selectActiveTab, useStore } from "../state/store";
import { connectedStore, session, table } from "../test/fixtures";
import { paletteHit } from "../test/paletteFixtures";
import { showTableIcons } from "../test/iconFixtures";
import { CommandPalette } from "./CommandPalette";
import { parsePrefix } from "./palette/parse";

const mocked = vi.mocked(api, true);

const hit = (key: string, source = "wp_wc_orders"): PaletteHit => paletteHit(source, key);
const objectTabs = () => useStore.getState().tabs.filter((t) => t.kind === "object");
const rulesInfo = (entities: string[]): RulesInfo => ({ prefix: "wp_", networkPrefix: "wp_", suggestedPrefix: null, appliedPresets: [], availablePresets: [], entities: entities.map((name) => ({ preset: "woocommerce", entity: { name, table: "wp_wc_orders" } })), tableEntities: {}, tableIcons: {} });

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

const openPalette = () => {
  useStore.getState().setPaletteOpen(true);
  render(<CommandPalette />);
  return screen.getByLabelText("Search everything");
};
const selectedOptions = () => screen.getAllByRole("option", { selected: true });
const optionTitles = () => screen.getAllByRole("option").map((o) => o.getAttribute("data-stage-label"));

describe("CommandPalette", () => {
  const scrollIntoView = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    Element.prototype.scrollIntoView = scrollIntoView;
    connectedStore();
    installCommands();
  });

  afterEach(() => {
    delete (Element.prototype as Partial<Element>).scrollIntoView;
  });

  describe("without a query", () => {
    beforeEach(() =>
      useStore.setState({
        recentNav: [
          { kind: "table", database: "db", table: "wp_posts" },
          { kind: "table", database: "db", table: "wp_options" },
        ],
      }),
    );

    it("lists recent items first under a Recent heading", () => {
      openPalette();
      expect(screen.getByText("Recent")).toBeInTheDocument();
      const options = screen.getAllByRole("option");
      expect(options[0]).toHaveTextContent("db.wp_posts");
      expect(options[1]).toHaveTextContent("db.wp_options");
    });

    it("hides the recent items once a query is typed", async () => {
      await userEvent.type(openPalette(), "zoom");
      expect(screen.queryByText("Recent")).not.toBeInTheDocument();
      expect(screen.queryByText("db.wp_posts")).not.toBeInTheDocument();
    });

    it("moves the selection with ArrowDown and ArrowUp and scrolls the active option into view", async () => {
      openPalette();
      expect(selectedOptions()).toHaveLength(1);
      expect(selectedOptions()[0]).toHaveTextContent("db.wp_posts");

      await userEvent.keyboard("{ArrowDown}");
      expect(selectedOptions()).toHaveLength(1);
      expect(selectedOptions()[0]).toHaveTextContent("db.wp_options");
      expect(scrollIntoView.mock.contexts.at(-1)).toBe(selectedOptions()[0]);

      await userEvent.keyboard("{ArrowUp}");
      expect(selectedOptions()[0]).toHaveTextContent("db.wp_posts");
      expect(scrollIntoView.mock.contexts.at(-1)).toBe(selectedOptions()[0]);
    });

    it("opens the selected recent table on Enter", async () => {
      openPalette();
      await userEvent.keyboard("{ArrowDown}{Enter}");
      const tab = useStore.getState().tabs.at(-1);
      expect(tab).toMatchObject({ kind: "table", table: "wp_options" });
      expect(useStore.getState().paletteOpen).toBe(false);
    });
  });

  describe("opening a table", () => {
    it("opens it in the preview tab on Enter", async () => {
      await userEvent.type(openPalette(), "t:wp_posts{Enter}");
      expect(useStore.getState().tabs.at(-1)).toMatchObject({ kind: "table", table: "wp_posts", preview: true });
      expect(useStore.getState().paletteOpen).toBe(false);
    });

    it("opens it pinned on ⌘↵", async () => {
      await userEvent.type(openPalette(), "t:wp_posts");
      await userEvent.keyboard("{Meta>}{Enter}{/Meta}");
      expect(useStore.getState().tabs.at(-1)).toMatchObject({ kind: "table", table: "wp_posts", preview: false });
      expect(useStore.getState().paletteOpen).toBe(false);
    });
  });

  describe("table search with t:", () => {
    it("ranks an exact name before a prefix before a contained name and badges views", async () => {
      connectedStore({ tables: [table("wp_posts"), table("x_posts", { isView: true }), table("posts_archive"), table("posts"), table("users")] });
      await userEvent.type(openPalette(), "t:posts");
      expect(optionTitles()).toEqual(["posts", "posts_archive", "wp_posts", "x_posts"]);
      const view = screen.getAllByRole("option")[3];
      expect(within(view).getByText("view")).toBeInTheDocument();
      expect(within(screen.getAllByRole("option")[2]).queryByText("view")).not.toBeInTheDocument();
    });

    it("lists at most 40 tables", async () => {
      connectedStore({ tables: Array.from({ length: 50 }, (_, i) => table(`table_${i}`)) });
      await userEvent.type(openPalette(), "t:");
      expect(screen.getAllByRole("option")).toHaveLength(40);
    });
  });

  describe("command search with >", () => {
    beforeEach(() => {
      registerCommands([
        { id: "test.zebraExact", title: "Zebra", scope: "global", run: () => {} },
        { id: "test.zebraPrefix", title: "Zebra crossing", scope: "global", run: () => {} },
        { id: "test.zebraContains", title: "Big zebra", scope: "global", run: () => {} },
        { id: "test.zebraHidden", title: "Zebra hidden", scope: "global", when: () => false, run: () => {} },
      ]);
    });

    it("lists commands only, without tables, and leaves out commands that are not available", async () => {
      await userEvent.type(openPalette(), ">");
      expect(screen.getByRole("option", { name: /Zoom in/ })).toBeInTheDocument();
      expect(screen.queryByRole("option", { name: /Apply pending changes/ })).not.toBeInTheDocument();
      expect(screen.queryByRole("option", { name: /wp_posts/ })).not.toBeInTheDocument();
    });

    it("ranks an exact title before a prefix before a contained text and skips unavailable commands", async () => {
      await userEvent.type(openPalette(), ">zebra");
      expect(optionTitles()).toEqual(["Zebra", "Zebra crossing", "Big zebra"]);
    });
  });

  it("closes on Escape", async () => {
    openPalette();
    await userEvent.keyboard("{Escape}");
    expect(useStore.getState().paletteOpen).toBe(false);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  describe("row search", () => {
    afterEach(() => {
      vi.useRealTimers();
    });

    it("searches rows for an id query after the debounce, without needing entities", async () => {
      mocked.paletteSearch.mockResolvedValue([hit("4711"), hit("4711", "wp_posts")]);
      await userEvent.type(openPalette(), "#4711");
      expect(await screen.findByRole("option", { name: "wp_wc_orders #4711" })).toBeInTheDocument();
      expect(screen.getByRole("option", { name: "wp_posts #4711" })).toBeInTheDocument();
      expect(screen.getByText("objects with id 4711")).toBeInTheDocument();
      expect(mocked.paletteSearch).toHaveBeenLastCalledWith("s1", "db", "#4711");
    });

    it("groups hits under their section headings", async () => {
      mocked.paletteSearch.mockResolvedValue([paletteHit("wp_wc_orders", "4711"), paletteHit("wp_wc_orders_meta.meta_value", "4712", "objects with value 4711", "wp_wc_orders")]);
      await userEvent.type(openPalette(), "=4711");
      await screen.findByRole("option", { name: "wp_wc_orders #4711" });
      expect(screen.getByText("objects with id 4711")).toBeInTheDocument();
      expect(screen.getByText("objects with value 4711")).toBeInTheDocument();
    });

    it("keeps a data-stage anchor per hit named after its address", async () => {
      mocked.paletteSearch.mockResolvedValue([hit("4711")]);
      await userEvent.type(openPalette(), "#4711");
      const option = await screen.findByRole("option", { name: "wp_wc_orders #4711" });
      expect(option).toHaveAttribute("data-stage", `palette:item:hit:${formatAddress(hit("4711").address)}`);
    });

    it("links the table of a hovered hit without provenance", async () => {
      mocked.paletteSearch.mockResolvedValue([hit("4711", "wp_posts")]);
      await userEvent.type(openPalette(), "#4711");
      await userEvent.hover(await screen.findByRole("option", { name: "wp_posts #4711" }));
      await vi.waitFor(() => expect(useStore.getState().hover).toEqual({ src: null, table: "wp_posts", linked: false }));
    });

    it("searches free text from 6 characters and not below", async () => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      const input = openPalette();
      await userEvent.type(input, "jane@");
      await vi.advanceTimersByTimeAsync(400);
      expect(mocked.paletteSearch).not.toHaveBeenCalled();

      await userEvent.type(input, "x");
      await vi.advanceTimersByTimeAsync(400);
      expect(mocked.paletteSearch).toHaveBeenCalledWith("s1", "db", "jane@x");
    });

    it("stops showing 'searching…' when the query drops below two characters while the search is pending", async () => {
      const pending = deferred<PaletteHit[]>();
      mocked.paletteSearch.mockReturnValueOnce(pending.promise);
      await userEvent.type(openPalette(), "=ab");
      await vi.waitFor(() => expect(mocked.paletteSearch).toHaveBeenCalled());
      expect(screen.getByText("searching…")).toBeInTheDocument();

      await userEvent.keyboard("{Backspace}");
      expect(screen.queryByText("searching…")).not.toBeInTheDocument();
      await act(async () => pending.resolve([hit("1")]));
      expect(screen.queryByRole("option", { name: "wp_wc_orders #1" })).not.toBeInTheDocument();
    });

    it("keeps exactly one option selected when a reply with fewer hits arrives, and Enter opens it", async () => {
      const second = deferred<PaletteHit[]>();
      mocked.paletteSearch.mockResolvedValueOnce([hit("120"), hit("121"), hit("122")]).mockReturnValueOnce(second.promise);
      await userEvent.type(openPalette(), "#12");
      await screen.findByRole("option", { name: "wp_wc_orders #122" });

      await userEvent.keyboard("3{ArrowDown}{ArrowDown}");
      expect(selectedOptions()[0]).toHaveAccessibleName("wp_wc_orders #122");

      await vi.waitFor(() => expect(mocked.paletteSearch).toHaveBeenLastCalledWith("s1", "db", "#123"));
      await act(async () => second.resolve([hit("123")]));
      expect(screen.getAllByRole("option")).toHaveLength(1);
      expect(selectedOptions()[0]).toHaveAccessibleName("wp_wc_orders #123");

      await userEvent.keyboard("{Enter}");
      await vi.waitFor(() => expect(selectActiveTab(useStore.getState())).toMatchObject({ kind: "object", address: { target: { kind: "row", table: "wp_wc_orders", key: "123" } } }));
    });

    it.each(["#12a", "@j"])("does not search rows for %s", async (query) => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      await userEvent.type(openPalette(), query);
      await vi.advanceTimersByTimeAsync(400);
      expect(mocked.paletteSearch).not.toHaveBeenCalled();
    });

    it.each([
      ["q:", "No saved queries."],
      ["#", "Type an id, e.g. #4711."],
    ])("explains the empty %s search", async (query, message) => {
      await userEvent.type(openPalette(), query);
      expect(screen.getByText(message)).toBeInTheDocument();
    });
  });

  describe("opening a hit", () => {
    beforeEach(() => mocked.paletteSearch.mockResolvedValue([hit("4712")]));

    it("opens an object tab on Enter when no object tab is active", async () => {
      await userEvent.type(openPalette(), "#4712");
      await screen.findByRole("option", { name: "wp_wc_orders #4712" });
      await userEvent.keyboard("{Enter}");
      expect(selectActiveTab(useStore.getState())).toMatchObject({ kind: "object", address: { target: { kind: "row", table: "wp_wc_orders", key: "4712" } } });
      expect(useStore.getState().paletteOpen).toBe(false);
    });

    it("drills into a new column on Enter when an object tab is active", async () => {
      useStore.getState().openObject(hit("4711").address);
      await userEvent.type(openPalette(), "#4712");
      await screen.findByRole("option", { name: "wp_wc_orders #4712" });
      await userEvent.keyboard("{Enter}");
      const tab = selectActiveTab(useStore.getState());
      expect(objectTabs()).toHaveLength(1);
      expect(tab).toMatchObject({ kind: "object", active: 1 });
      expect(tab?.kind === "object" && tab.columns).toHaveLength(2);
    });

    it("opens a new object tab on ⌘↵ even when an object tab is active", async () => {
      useStore.getState().openObject(hit("4711").address);
      await userEvent.type(openPalette(), "#4712");
      await screen.findByRole("option", { name: "wp_wc_orders #4712" });
      await userEvent.keyboard("{Meta>}{Enter}{/Meta}");
      expect(objectTabs()).toHaveLength(2);
      expect(selectActiveTab(useStore.getState())).toMatchObject({ kind: "object", address: { target: { kind: "row", key: "4712" } } });
    });

    it("peeks the selected hit while ⌥ is held", async () => {
      await userEvent.type(openPalette(), "#4712");
      await screen.findByRole("option", { name: "wp_wc_orders #4712" });
      const user = userEvent.setup();
      await user.keyboard("{Alt>}");
      expect(await screen.findByRole("complementary", { name: "Peek wp_wc_orders 4712" })).toBeInTheDocument();
      await user.keyboard("{/Alt}");
      await vi.waitFor(() => expect(screen.queryByRole("complementary", { name: /Peek/ })).not.toBeInTheDocument());
    });
  });

  describe("saved queries", () => {
    const failed: SavedEntry = { id: "failed-orders", title: "Failed orders", sql: "SELECT * FROM wp_wc_orders WHERE status = 'wc-failed'", pin: { show: "count" }, origin: { kind: "project" }, readOnly: false };
    const stale: SavedEntry = { id: "stale-sessions", title: "Stale sessions", sql: "SELECT * FROM wp_usermeta", pin: null, origin: { kind: "global" }, readOnly: false };
    const preset: SavedEntry = { id: "abandoned-carts", title: "Abandoned carts", sql: "SELECT * FROM wp_woocommerce_sessions", pin: null, origin: { kind: "preset", preset: "woocommerce" }, readOnly: true };

    beforeEach(() => mocked.savedQueries.mockResolvedValue([failed, stale, preset]));

    it("lists each query with where it is stored and a badge for pinned ones", async () => {
      await userEvent.type(openPalette(), "q:");
      const pinned = await screen.findByRole("option", { name: /Failed orders/ });
      expect(pinned).toHaveTextContent("project");
      expect(pinned).toHaveTextContent("pinned");
      expect(screen.getByRole("option", { name: /Stale sessions/ })).toHaveTextContent("global");
      expect(screen.getByRole("option", { name: /Stale sessions/ })).not.toHaveTextContent("pinned");
      expect(screen.getByRole("option", { name: /Abandoned carts/ })).toHaveTextContent("preset:woocommerce");
    });

    it("narrows the list to titles matching the text after q:", async () => {
      await userEvent.type(openPalette(), "q:stale");
      expect(await screen.findByRole("option", { name: /Stale sessions/ })).toBeInTheDocument();
      expect(screen.queryByRole("option", { name: /Failed orders/ })).not.toBeInTheDocument();
    });

    it("matches a query by its id when the title does not match", async () => {
      await userEvent.type(openPalette(), "q:stale-sessions");
      expect(await screen.findByRole("option", { name: /Stale sessions/ })).toBeInTheDocument();
      expect(screen.queryByRole("option", { name: /Failed orders/ })).not.toBeInTheDocument();
    });

    it("opens the selected query in a query tab with its SQL and runs it on Enter", async () => {
      await userEvent.type(openPalette(), "q:failed");
      await screen.findByRole("option", { name: /Failed orders/ });
      await userEvent.keyboard("{Enter}");
      expect(selectActiveTab(useStore.getState())).toMatchObject({ kind: "query", title: "Failed orders", sql: failed.sql });
      expect(useStore.getState().paletteOpen).toBe(false);
      await vi.waitFor(() => expect(mocked.execute).toHaveBeenCalledWith("s1", failed.sql, false));
    });

    it("opens a preset query as an unsaved copy", async () => {
      await userEvent.type(openPalette(), "q:carts");
      await screen.findByRole("option", { name: /Abandoned carts/ });
      await userEvent.keyboard("{Enter}");
      expect(selectActiveTab(useStore.getState())).toMatchObject({ kind: "query", title: "copy of Abandoned carts", sql: preset.sql, saved: { origin: { kind: "preset", preset: "woocommerce" } } });
    });
  });

  describe("addresses", () => {
    it("lists a typed row address and opens it on Enter", async () => {
      await userEvent.type(openPalette(), `databee://${session.slug}/db/wp_posts/1`);
      expect(screen.getByText("Address")).toBeInTheDocument();
      expect(screen.getByRole("option", { name: /databee:\/\/shop\/db\/wp_posts\/1/ })).toBeInTheDocument();
      await userEvent.keyboard("{Enter}");
      expect(selectActiveTab(useStore.getState())).toMatchObject({ kind: "object", address: { target: { kind: "row", table: "wp_posts", key: "1" } } });
    });

    it("lists no Address for a malformed address and says it is not one yet", async () => {
      await userEvent.type(openPalette(), "databee://x");
      expect(screen.queryByText("Address")).not.toBeInTheDocument();
      expect(screen.queryByRole("option")).not.toBeInTheDocument();
      expect(screen.getByText("Not an address yet.")).toBeInTheDocument();
    });

    it("opens the palette seeded with the value when a find address is opened", async () => {
      render(<CommandPalette />);
      await act(() => useStore.getState().openAddress(`databee://${session.slug}/db/?find=x`));
      expect(await screen.findByLabelText("Search everything")).toHaveValue("=x");
    });
  });

  describe("table icons", () => {
    beforeEach(() => showTableIcons({ wp_posts: "file-text" }));

    it("draws the icon of the table in a table item and the default table icon for one without", async () => {
      await userEvent.type(openPalette(), "t:wp_");
      expect(screen.getByRole("option", { name: /wp_posts/ }).querySelector("[data-icon=file-text]")).not.toBeNull();
      expect(screen.getByRole("option", { name: /wp_options/ }).querySelector("[data-icon=table]")).not.toBeNull();
    });

    it("draws the icon of the table in a recent table item", () => {
      useStore.setState({ recentNav: [{ kind: "table", database: "db", table: "wp_posts" }] });
      openPalette();
      expect(screen.getByRole("option", { name: /wp_posts/ }).querySelector("[data-icon=file-text]")).not.toBeNull();
    });

    it("draws the icon of the hit's table before its source", async () => {
      mocked.paletteSearch.mockResolvedValue([hit("1", "wp_posts"), hit("1", "wp_options")]);
      await userEvent.type(openPalette(), "#1");
      const posts = await screen.findByRole("option", { name: "wp_posts #1" });
      expect(posts.querySelector("[data-icon=file-text]")).not.toBeNull();
      expect(screen.getByRole("option", { name: "wp_options #1" }).querySelector("[data-icon=table]")).not.toBeNull();
    });
  });

  describe("footer", () => {
    it("says which tables # reads when the database has no entities", async () => {
      await userEvent.type(openPalette(), "#4");
      expect(screen.getByText("no entities: # reads tables under 1M estimated rows")).toBeInTheDocument();
    });

    it("omits that note when the database has entities", async () => {
      useStore.setState({ rulesInfo: { [projectKey("s1", "db")]: rulesInfo(["order"]) } });
      await userEvent.type(openPalette(), "#4");
      expect(screen.queryByText(/no entities/)).not.toBeInTheDocument();
    });
  });
});

describe("parsePrefix", () => {
  it.each([
    ["q:orders", "q:", "orders"],
    ["=66M12696JX411763D", "=", "66M12696JX411763D"],
    ["  #4711 ", "#", "4711"],
    ["jane", null, "jane"],
  ])("splits %j into prefix and needle", (input, prefix, needle) => {
    expect(parsePrefix(input)).toEqual({ prefix, needle });
  });

  it("keeps the whole text of a databee:// address as the needle", () => {
    expect(parsePrefix("databee://shop/db/wp_posts/1")).toEqual({ prefix: "databee://", needle: "databee://shop/db/wp_posts/1" });
  });
});
