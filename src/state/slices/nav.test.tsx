import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import App from "../../App";
import { api } from "../../api";
import { installCommands, runCommand } from "../../commands";
import { Toasts } from "../../components/Toasts";
import { address, formatAddress } from "../../lib/address";
import { browsePage, columnMeta, connectedStore, queryTab, session, tableTab, text, value } from "../../test/fixtures";
import { objectDocument } from "../../test/objectFixtures";
import { wholeTable, type NavEntry } from "./nav";
import { selectActiveTab, useStore } from "../store";

const mocked = vi.mocked(api, true);

const pages = [{ column: "post_type", value: "page" }];
const row7 = [{ column: "ID", value: "7" }];

const activeTable = () => {
  const tab = selectActiveTab(useStore.getState());
  if (tab?.kind !== "table") throw new Error("the active tab is not a table tab");
  return tab;
};
const addressOfActive = () => formatAddress(activeTable().address);

/** Opens the pages of wp_posts, then row 7 in the same tab, with the primary key known. */
async function openPagesThenRow() {
  await useStore.getState().loadStructure(session.id, "db", "wp_posts");
  act(() => {
    useStore.getState().openTableTab("db", "wp_posts", "data", undefined, { where: pages });
    useStore.getState().showRows(activeTable().id, { database: "db", table: "wp_posts", where: row7 });
  });
}

describe("tab history", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [] });
    installCommands();
    mocked.describeTable.mockResolvedValue({ columns: [], indexes: [], primaryKey: ["ID"], foreignKeys: [], ddl: null });
  });

  it("derives a row address from the single-column primary key", async () => {
    await openPagesThenRow();
    expect(activeTable().where).toEqual(row7);
    expect(addressOfActive()).toBe("databee://shop/db/wp_posts/7?view=rows");
  });

  it("goes back to the earlier rows and forward again", async () => {
    await openPagesThenRow();

    expect(runCommand("nav.back")).toBe(true);
    await waitFor(() => expect(activeTable().where).toEqual(pages));
    expect(addressOfActive()).toBe("databee://shop/db/wp_posts?post_type=page");

    expect(runCommand("nav.forward")).toBe(true);
    await waitFor(() => expect(activeTable().where).toEqual(row7));
  });

  it("can no longer go back once the history is used up", async () => {
    await openPagesThenRow();
    runCommand("nav.back");
    await waitFor(() => expect(activeTable().where).toEqual(pages));
    expect(runCommand("nav.back")).toBe(false);
  });

  it("is not runnable without history", () => {
    act(() => useStore.getState().openTableTab("db", "wp_posts"));
    expect(runCommand("nav.back")).toBe(false);
    expect(runCommand("nav.forward")).toBe(false);
  });

  it("drops the forward history when other rows are shown after going back", async () => {
    await openPagesThenRow();
    runCommand("nav.back");
    await waitFor(() => expect(activeTable().where).toEqual(pages));
    act(() => useStore.getState().showRows(activeTable().id, { database: "db", table: "wp_posts", where: [{ column: "ID", value: "9" }] }));
    expect(runCommand("nav.forward")).toBe(false);
    expect(runCommand("nav.back")).toBe(true);
  });

  it("records nothing when the same rows are shown again", () => {
    act(() => {
      useStore.getState().openTableTab("db", "wp_posts", "data", undefined, { where: pages });
      useStore.getState().showRows(activeTable().id, { database: "db", table: "wp_posts", where: pages });
    });
    expect(runCommand("nav.back")).toBe(false);
  });

  it("keeps a typed SQL filter out of the address and the history", () => {
    act(() => useStore.getState().openTableTab("db", "wp_posts"));
    const { id } = activeTable();
    act(() => useStore.getState().setTableFilter(id, "post_status = 'draft'"));
    expect(activeTable().filter).toBe("post_status = 'draft'");
    expect(addressOfActive()).toBe("databee://shop/db/wp_posts");
    expect(runCommand("nav.back")).toBe(false);
  });

  it("clears the typed SQL filter when history shows other rows", async () => {
    await openPagesThenRow();
    runCommand("nav.back");
    await waitFor(() => expect(activeTable().where).toEqual(pages));
    act(() => useStore.getState().setTableFilter(activeTable().id, "post_status = 'draft'"));
    runCommand("nav.forward");
    await waitFor(() => expect(activeTable().where).toEqual(row7));
    expect(activeTable().filter).toBeUndefined();
  });

});

describe("history with the rendered table view", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [] });
    mocked.describeTable.mockResolvedValue({ columns: [], indexes: [], primaryKey: ["ID"], foreignKeys: [], ddl: null });
    mocked.browseTable.mockImplementation(async (_s, req) =>
      browsePage({
        table: req.table,
        columnsFilter: req.where?.length ? { kind: "columns", sql: "", label: req.where.map((c) => `${c.column} = ${c.value}`).join(" AND ") } : null,
      }),
    );
  });

  it("restores a cleared column filter on ⌘[", async () => {
    render(<App />);
    await screen.findByText("wp_options");
    act(() => useStore.getState().openTableTab("db", "wp_posts", "data", undefined, { where: pages }));
    expect(await screen.findByText("post_type = page")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Clear column filter" }));
    await waitFor(() => expect(screen.queryByText("post_type = page")).not.toBeInTheDocument());

    fireEvent.keyDown(window, { key: "[", code: "BracketLeft", metaKey: true });
    expect(await screen.findByText("post_type = page")).toBeInTheDocument();
  });
});

describe("openAddress", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [] });
    mocked.describeTable.mockResolvedValue({ columns: [], indexes: [], primaryKey: ["ID"], foreignKeys: [], ddl: null });
    render(<Toasts />);
  });

  it("opens a table tab filtered by a table address's columns", async () => {
    await act(() => useStore.getState().openAddress("databee://shop/db/wp_posts?ID=7"));
    expect(activeTable()).toMatchObject({ table: "wp_posts", where: row7, title: "wp_posts (filtered)" });
  });

  it("opens a row address as an object tab in the view it names, without describing the table", async () => {
    await act(() => useStore.getState().openAddress("databee://shop/db/wp_posts/7?view=rows"));
    expect(selectActiveTab(useStore.getState())).toMatchObject({
      kind: "object",
      columns: [{ kind: "object", view: "rows" }],
      address: { view: "rows", target: { kind: "row", table: "wp_posts", key: "7" } },
    });
    expect(mocked.describeTable).not.toHaveBeenCalled();
  });

  it("opens a bare table address unfiltered", async () => {
    await act(() => useStore.getState().openAddress("  databee://shop/db/wp_options  "));
    expect(activeTable()).toMatchObject({ table: "wp_options", where: [] });
  });

  it("opens a pinned tab by default", async () => {
    await act(() => useStore.getState().openAddress("databee://shop/db/wp_posts"));
    expect(activeTable().preview).toBe(false);
  });

  it("opens a preview tab when asked, and the next preview replaces it", async () => {
    await act(() => useStore.getState().openAddress("databee://shop/db/wp_posts", { preview: true }));
    expect(activeTable()).toMatchObject({ table: "wp_posts", preview: true });

    await act(() => useStore.getState().openAddress("databee://shop/db/wp_options", { preview: true }));
    expect(useStore.getState().tabs).toHaveLength(1);
    expect(activeTable()).toMatchObject({ table: "wp_options", preview: true });
  });

  it("reports a connection that is not open", async () => {
    await act(() => useStore.getState().openAddress("databee://nope/db/wp_posts"));
    expect(await screen.findByRole("alert")).toHaveTextContent("No open connection is named nope");
    expect(useStore.getState().tabs).toEqual([]);
  });

  it("reports a malformed address", async () => {
    await act(() => useStore.getState().openAddress("shop/db/wp_posts"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Not a DataBee address");
    expect(useStore.getState().tabs).toEqual([]);
  });

  it("opens a row address of a composite-key table as an object tab", async () => {
    mocked.describeTable.mockResolvedValue({ columns: [], indexes: [], primaryKey: ["a", "b"], foreignKeys: [], ddl: null });
    await act(() => useStore.getState().openAddress("databee://shop/db/wp_postmeta/7"));
    expect(selectActiveTab(useStore.getState())).toMatchObject({ kind: "object", address: { target: { kind: "row", table: "wp_postmeta", key: "7" } } });
  });

  describe("a saved query address", () => {
    const failed = { id: "failed-orders", title: "Failed orders", sql: "SELECT * FROM wp_wc_orders WHERE status = 'wc-failed'", pin: null, origin: { kind: "project" as const }, readOnly: false };

    it("opens the saved query in a query tab and runs it", async () => {
      mocked.savedQueries.mockResolvedValue([failed]);
      await act(() => useStore.getState().openAddress("databee://shop/db/q/failed-orders"));
      expect(selectActiveTab(useStore.getState())).toMatchObject({ kind: "query", title: "Failed orders", sql: failed.sql });
      await waitFor(() => expect(mocked.execute).toHaveBeenCalledWith("s1", failed.sql, false));
    });

    it("reports an id that names no saved query", async () => {
      mocked.savedQueries.mockResolvedValue([failed]);
      await act(() => useStore.getState().openAddress("databee://shop/db/q/nope"));
      expect(await screen.findByRole("alert")).toHaveTextContent("names no saved query");
      expect(useStore.getState().tabs).toEqual([]);
    });
  });
});

describe("filterRows", () => {
  const draft = { column: "post_status", value: "draft" };

  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [] });
    installCommands();
    act(() => useStore.getState().openTableTab("db", "wp_posts", "data", undefined, { where: pages }));
  });

  it("adds a match to the rows already filtered", () => {
    act(() => useStore.getState().filterRows(activeTable().id, draft));
    expect(activeTable().where).toEqual([...pages, draft]);
  });

  it("replaces a match on the same column, whatever its case", () => {
    act(() => useStore.getState().filterRows(activeTable().id, draft));
    act(() => useStore.getState().filterRows(activeTable().id, { column: "POST_TYPE", value: "post" }));
    expect(activeTable().where).toEqual([draft, { column: "POST_TYPE", value: "post" }]);
  });

  it("records the earlier rows for back", async () => {
    act(() => useStore.getState().filterRows(activeTable().id, draft));
    expect(runCommand("nav.back")).toBe(true);
    await waitFor(() => expect(activeTable().where).toEqual(pages));
  });

  it("matches NULL", () => {
    act(() => useStore.getState().filterRows(activeTable().id, { column: "post_parent", value: null }));
    expect(addressOfActive()).toBe("databee://shop/db/wp_posts?post_type=page&post_parent");
  });

  it("does nothing for a tab that is not a table tab", () => {
    const before = useStore.getState().tabs;
    act(() => useStore.getState().filterRows("missing", draft));
    expect(useStore.getState().tabs).toBe(before);
  });
});

describe("navigateTab", () => {
  const post7 = address("shop", "db", { kind: "row", table: "wp_posts", key: "7" });
  const options = address("shop", "db", { kind: "table", table: "wp_options", filter: [] });
  const entries = (tab: { nav: { back: NavEntry[]; forward: NavEntry[] } }, direction: "back" | "forward") =>
    tab.nav[direction].map((e) => [e.kind, formatAddress(e.address)]);
  const tabById = (id: string) => useStore.getState().tabs.find((t) => t.id === id);

  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [tableTab({ id: "t0", table: "wp_options" }), tableTab({ id: "t1", preview: true })] });
    useStore.setState({ activeTabId: "t1" });
    installCommands();
    mocked.document.mockResolvedValue(objectDocument());
  });

  it("turns a table tab into an object tab of the row in place, keeping id, position and preview", () => {
    act(() => useStore.getState().navigateTab("t1", post7));
    expect(useStore.getState().tabs.map((t) => t.id)).toEqual(["t0", "t1"]);
    expect(tabById("t1")).toMatchObject({
      kind: "object",
      preview: true,
      title: "wp_posts #7",
      address: { target: { kind: "row", table: "wp_posts", key: "7" } },
      columns: [{ kind: "object", address: { target: { key: "7" } } }],
      active: 0,
    });
    expect(useStore.getState().activeTabId).toBe("t1");
  });

  it("turns an object tab into the whole table in the data view", () => {
    act(() => useStore.getState().navigateTab("t1", post7));
    act(() => useStore.getState().navigateTab("t1", wholeTable(post7)));
    expect(tabById("t1")).toMatchObject({ kind: "table", table: "wp_posts", where: [], view: "data", preview: true, title: "wp_posts" });
  });

  it("moves a table tab to another table", () => {
    act(() => useStore.getState().navigateTab("t1", options));
    expect(tabById("t1")).toMatchObject({ kind: "table", table: "wp_options", where: [] });
    expect(entries(tabById("t1")!, "back")).toEqual([["table", "databee://shop/db/wp_posts"]]);
  });

  it("records the location it left for back and clears the forward history", async () => {
    act(() => useStore.getState().navigateTab("t1", post7));
    await act(() => useStore.getState().navBack());
    expect(entries(tabById("t1")!, "forward")).toEqual([["object", "databee://shop/db/wp_posts/7"]]);
    act(() => useStore.getState().navigateTab("t1", options));
    expect(entries(tabById("t1")!, "forward")).toEqual([]);
    expect(entries(tabById("t1")!, "back")).toEqual([["table", "databee://shop/db/wp_posts"]]);
  });

  it("clears the grid selection of the tab it moves", () => {
    act(() => {
      useStore.getState().selectCell({
        row: 0,
        col: 0,
        column: columnMeta("ID", "BIGINT"),
        cell: text("7"),
        source: { sessionId: "s1", resultId: 1 },
        value: value({ raw: text("7") }),
      });
    });
    expect(useStore.getState().selectedCell?.tabId).toBe("t1");
    act(() => useStore.getState().navigateTab("t1", post7));
    expect(useStore.getState().selectedCell).toBeNull();
  });

  it.each([
    ["a home address", address("shop", "db", { kind: "home" })],
    ["a project address", address("shop", "db", { kind: "project" })],
  ])("ignores %s", (_name, target) => {
    const before = useStore.getState().tabs;
    act(() => useStore.getState().navigateTab("t1", target));
    expect(useStore.getState().tabs).toBe(before);
  });

  it("ignores a query tab and an unknown tab", () => {
    act(() => useStore.getState().openQueryTab());
    const query = selectActiveTab(useStore.getState())!;
    const before = useStore.getState().tabs;
    act(() => useStore.getState().navigateTab(query.id, post7));
    act(() => useStore.getState().navigateTab("missing", post7));
    expect(useStore.getState().tabs).toBe(before);
  });

  it("goes back from the row to the table and forward to the row again", async () => {
    act(() => useStore.getState().navigateTab("t1", post7));

    await act(() => useStore.getState().navBack());
    expect(tabById("t1")).toMatchObject({ kind: "table", table: "wp_posts", where: [], preview: true });

    await act(() => useStore.getState().navForward());
    expect(tabById("t1")).toMatchObject({ kind: "object", address: { target: { key: "7" } } });
  });

  it("restores the drilled column stack when going back from the table to the object", async () => {
    act(() => useStore.getState().navigateTab("t1", post7));
    act(() => useStore.getState().drill("t1", 0, address("shop", "db", { kind: "row", table: "wp_users", key: "12" })));
    expect(tabById("t1")).toMatchObject({ columns: [{}, {}], active: 1 });

    act(() => useStore.getState().navigateTab("t1", wholeTable(post7)));
    expect(tabById("t1")?.kind).toBe("table");

    await act(() => useStore.getState().navBack());
    expect(tabById("t1")).toMatchObject({
      kind: "object",
      columns: [{ address: { target: { table: "wp_posts" } } }, { address: { target: { table: "wp_users" } } }],
      active: 1,
    });
  });
});

describe("wholeTable", () => {
  it("turns a row address into the unfiltered table", () => {
    const row = address("shop", "db", { kind: "row", table: "wp_posts", key: "7" });
    expect(formatAddress(wholeTable({ ...row, view: "rows" }))).toBe("databee://shop/db/wp_posts");
  });

  it("leaves a filtered table address as it is", () => {
    const filtered = address("shop", "db", { kind: "table", table: "wp_posts", filter: pages });
    expect(wholeTable(filtered)).toBe(filtered);
  });
});

describe("renameConnectionSlug", () => {
  const earlier = { kind: "table" as const, address: tableTab().address };

  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [queryTab(), tableTab({ nav: { back: [earlier], forward: [earlier] } })] });
  });

  it("sends the identity and slug to Rust", async () => {
    await useStore.getState().renameConnectionSlug(session.identity, "store");
    expect(mocked.renameConnectionSlug).toHaveBeenCalledWith(session.identity, "store");
  });

  it("rewrites the session, the tab addresses and their history", async () => {
    await useStore.getState().renameConnectionSlug(session.identity, "store");
    const { sessions, tabs } = useStore.getState();
    expect(sessions[0].slug).toBe("store");
    const [query, table] = tabs;
    expect(query.address && formatAddress(query.address)).toBe("databee://store/db/");
    expect(table.address && formatAddress(table.address)).toBe("databee://store/db/wp_posts");
    expect(table.nav.back.map((e) => formatAddress(e.address))).toEqual(["databee://store/db/wp_posts"]);
    expect(table.nav.forward.map((e) => formatAddress(e.address))).toEqual(["databee://store/db/wp_posts"]);
  });

  it("reloads the accents, which Rust keys by the slug", async () => {
    useStore.setState({ accents: { "shop/db": "gold" } });
    mocked.projectAccents.mockResolvedValueOnce({ "store/db": "gold" });
    await useStore.getState().renameConnectionSlug(session.identity, "store");
    await waitFor(() => expect(useStore.getState().accents).toEqual({ "store/db": "gold" }));
  });

  it("keeps the accents when Rust rejects the slug", async () => {
    useStore.setState({ accents: { "shop/db": "gold" } });
    mocked.renameConnectionSlug.mockRejectedValueOnce({ kind: "invalid", message: "Slug is taken", code: null, account: null });
    await expect(useStore.getState().renameConnectionSlug(session.identity, "taken")).rejects.toBeDefined();
    expect(mocked.projectAccents).not.toHaveBeenCalled();
    expect(useStore.getState().accents).toEqual({ "shop/db": "gold" });
  });

  it("changes nothing when Rust rejects the slug", async () => {
    mocked.renameConnectionSlug.mockRejectedValueOnce({ kind: "invalid", message: "Slug is taken", code: null, account: null });
    await expect(useStore.getState().renameConnectionSlug(session.identity, "taken")).rejects.toMatchObject({ message: "Slug is taken" });
    expect(useStore.getState().sessions[0].slug).toBe("shop");
    expect(useStore.getState().tabs[1]).toMatchObject({ address: { connection: "shop" } });
  });
});
