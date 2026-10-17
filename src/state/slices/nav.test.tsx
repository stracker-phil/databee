import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import App from "../../App";
import { api } from "../../api";
import { installCommands, runCommand } from "../../commands";
import { Toasts } from "../../components/Toasts";
import { formatAddress } from "../../lib/address";
import { browsePage, connectedStore, queryTab, session, tableTab } from "../../test/fixtures";
import { wpStore } from "../../test/wpFixtures";
import { selectActiveTab, selectWpRoute, useStore } from "../store";

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
    useStore.getState().openRow("wp_posts", "ID", "7");
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
    act(() => useStore.getState().openRow("wp_posts", "ID", "9"));
    expect(runCommand("nav.forward")).toBe(false);
    expect(runCommand("nav.back")).toBe(true);
  });

  it("records nothing when the same rows are shown again", () => {
    act(() => {
      useStore.getState().openTableTab("db", "wp_posts", "data", undefined, { where: pages });
      useStore.getState().openRow("wp_posts", "post_type", "page");
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

  describe("in WordPress mode", () => {
    beforeEach(() => {
      wpStore();
      installCommands();
    });

    it("moves through the WordPress history instead", () => {
      act(() => useStore.getState().navigate({ screen: "list", list: "orders" }));
      expect(runCommand("nav.back")).toBe(true);
      expect(selectWpRoute(useStore.getState())).toEqual({ screen: "dashboard" });
      expect(runCommand("nav.forward")).toBe(true);
      expect(selectWpRoute(useStore.getState())).toEqual({ screen: "list", list: "orders" });
    });

    it("is not runnable on the first screen", () => {
      expect(runCommand("nav.back")).toBe(false);
    });
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

  it("resolves a row address to its primary key column", async () => {
    await act(() => useStore.getState().openAddress("databee://shop/db/wp_posts/7?view=rows"));
    expect(activeTable()).toMatchObject({ table: "wp_posts", where: row7 });
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

  it("reports a row address for a table without a single-column primary key", async () => {
    mocked.describeTable.mockResolvedValue({ columns: [], indexes: [], primaryKey: ["a", "b"], foreignKeys: [], ddl: null });
    await act(() => useStore.getState().openAddress("databee://shop/db/wp_postmeta/7"));
    expect(await screen.findByRole("alert")).toHaveTextContent("wp_postmeta has no single-column primary key");
    expect(useStore.getState().tabs).toEqual([]);
  });

  it("says that other address kinds do not open yet", async () => {
    await act(() => useStore.getState().openAddress("databee://shop/db/"));
    expect(await screen.findByRole("status")).toHaveTextContent("cannot be opened yet");
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

describe("renameConnectionSlug", () => {
  const earlier = tableTab().address;

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
    expect(table.nav.back.map(formatAddress)).toEqual(["databee://store/db/wp_posts"]);
    expect(table.nav.forward.map(formatAddress)).toEqual(["databee://store/db/wp_posts"]);
  });

  it("changes nothing when Rust rejects the slug", async () => {
    mocked.renameConnectionSlug.mockRejectedValueOnce({ kind: "invalid", message: "Slug is taken", code: null, account: null });
    await expect(useStore.getState().renameConnectionSlug(session.identity, "taken")).rejects.toMatchObject({ message: "Slug is taken" });
    expect(useStore.getState().sessions[0].slug).toBe("shop");
    expect(useStore.getState().tabs[1]).toMatchObject({ address: { connection: "shop" } });
  });
});
