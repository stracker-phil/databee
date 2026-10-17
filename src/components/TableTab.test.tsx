import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api, EMPTY_UI_STATE, type BrowsePage, type TableStructure } from "../api";
import { useStore, type TableTab as TableTabState } from "../state/store";
import { connectedStore, num, resultSet, session, tableTab, text } from "../test/fixtures";
import { TableTab } from "./TableTab";

const mocked = vi.mocked(api, true);

const structure: TableStructure = {
  columns: [
    { name: "ID", columnType: "bigint unsigned", nullable: false, default: null, key: "PRI", extra: "auto_increment", comment: "", autoIncrement: true, generated: false },
    { name: "post_title", columnType: "text", nullable: false, default: null, key: "", extra: "", comment: "", autoIncrement: false, generated: false },
  ],
  indexes: [{ name: "PRIMARY", unique: true, columns: ["ID"] }],
  primaryKey: ["ID"],
  ddl: "CREATE TABLE wp_posts (...)",
};

const tab: TableTabState = tableTab();

describe("TableTab", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
    useStore.setState({ tabs: [tab], activeTabId: tab.id });
    mocked.describeTable.mockResolvedValue(structure);
    mocked.browseTable.mockResolvedValue({ result: resultSet(), hasMore: false, limit: 500, filter: null, columnsFilter: null });
  });

  it("sends the primary key with the filter and shows the ID interpretation as a chip", async () => {
    mocked.browseTable.mockResolvedValue({
      result: resultSet(),
      hasMore: false,
      limit: 500,
      filter: { kind: "ids", sql: "`ID` IN (3229, 3227)", label: "ID in (3229, 3227)" },
      columnsFilter: null,
    });
    const Live = () => <TableTab tab={useStore((s) => s.tabs[0]) as TableTabState} />;
    render(<Live />);
    const where = screen.getByLabelText("WHERE filter");
    await waitFor(() => expect(mocked.browseTable).toHaveBeenLastCalledWith(session.id, expect.objectContaining({ primaryKey: "ID" })));
    await userEvent.type(where, "3229, 3227{Enter}");
    await waitFor(() => expect(mocked.browseTable).toHaveBeenLastCalledWith(session.id, expect.objectContaining({ filter: "3229, 3227", primaryKey: "ID" })));
    const chip = await screen.findByRole("button", { name: /ID in \(3229, 3227\)/ });
    await userEvent.click(chip);
    expect(screen.getByText("`ID` IN (3229, 3227)")).toBeInTheDocument();
  });

  it("shows no chip for plain SQL filters", async () => {
    mocked.browseTable.mockResolvedValue({ result: resultSet(), hasMore: false, limit: 500, filter: { kind: "sql", sql: "post_type = 'page'", label: "post_type = 'page'" }, columnsFilter: null });
    render(<TableTab tab={tab} />);
    await screen.findByText("Hello world");
    expect(screen.queryByRole("button", { name: /ID in/ })).not.toBeInTheDocument();
  });

  it("persists column widths per connection and table after a resize", async () => {
    render(<TableTab tab={tab} />);
    await screen.findByText("Hello world");
    const handle = screen.getByRole("separator", { name: "Resize post_title" });
    fireEvent.mouseDown(handle, { clientX: 100 });
    fireEvent.mouseMove(window, { clientX: 160 });
    fireEvent.mouseUp(window);
    const saved = useStore.getState().uiState[session.identity]?.tables["db.wp_posts"]?.widths;
    expect(saved).toBeDefined();
    expect(saved!.post_title).toBeGreaterThan(saved!.ID);
    const header = screen.getByRole("columnheader", { name: /post_title/ });
    expect(header.style.width).toBe("var(--col-1)");
    await waitFor(() => expect(mocked.setUiState).toHaveBeenCalledWith(session.identity, expect.objectContaining({ tables: { "db.wp_posts": { widths: saved } } })), { timeout: 1500 });
  });

  it("applies saved widths and auto-fits on double-click", async () => {
    useStore.setState({ uiState: { [session.identity]: { ...EMPTY_UI_STATE, tables: { "db.wp_posts": { widths: { ID: 300, post_title: 90 } } } } } });
    render(<TableTab tab={tab} />);
    await screen.findByText("Hello world");
    const grid = screen.getByRole("grid");
    expect(grid.style.getPropertyValue("--col-0")).toBe("300px");
    fireEvent.doubleClick(screen.getByRole("separator", { name: "Resize ID" }));
    const fitted = parseFloat(grid.style.getPropertyValue("--col-0"));
    expect(fitted).toBeLessThan(300);
    expect(fitted).toBeGreaterThanOrEqual(39);
  });

  describe("opening", () => {
    it("describes the table once and browses it once", async () => {
      render(<TableTab tab={tab} />);
      await screen.findByText("Hello world");
      expect(mocked.describeTable).toHaveBeenCalledTimes(1);
      expect(mocked.browseTable).toHaveBeenCalledTimes(1);
    });

    it("sends the primary key with the very first browse of a tab opened with an ID filter", async () => {
      render(<TableTab tab={{ ...tab, filter: "3229" }} />);
      await screen.findByText("Hello world");
      expect(mocked.browseTable).toHaveBeenNthCalledWith(1, session.id, expect.objectContaining({ filter: "3229", primaryKey: "ID" }));
    });

    it("does not describe the table again when switching Data, Structure and Data", async () => {
      useStore.setState({ tabs: [tab] });
      const Live = () => <TableTab tab={useStore((s) => s.tabs[0]) as TableTabState} />;
      render(<Live />);
      await screen.findByText("Hello world");
      await userEvent.click(screen.getByRole("button", { name: "Structure" }));
      await screen.findByRole("heading", { name: "Columns" });
      await userEvent.click(screen.getByRole("button", { name: "Data" }));
      await screen.findByText("Hello world");
      expect(mocked.describeTable).toHaveBeenCalledTimes(1);
    });
  });

  describe("opening a row", () => {
    const Active = () => {
      const active = useStore((s) => s.tabs.find((t) => t.id === s.activeTabId));
      return active?.kind === "table" ? <TableTab tab={active} /> : null;
    };
    const rowFilter = { kind: "columns" as const, sql: "`ID` = 7", label: "ID = 7" };

    beforeEach(() => {
      useStore.setState({ tabs: [], activeTabId: null });
      mocked.browseTable.mockResolvedValue({ result: resultSet(), hasMore: false, limit: 500, filter: null, columnsFilter: rowFilter });
    });

    it("requests the row by column equality and shows the filter Rust reports", async () => {
      useStore.getState().openRow("wp_posts", "ID", "7");
      render(<Active />);
      expect(await screen.findByText("ID = 7")).toBeInTheDocument();
      expect(mocked.browseTable).toHaveBeenLastCalledWith(session.id, expect.objectContaining({ table: "wp_posts", where: [{ column: "ID", value: "7" }] }));
    });

    it("passes a null value through as null", async () => {
      useStore.getState().openRow("wp_posts", "post_parent", null);
      render(<Active />);
      await waitFor(() => expect(mocked.browseTable).toHaveBeenCalled());
      expect(mocked.browseTable).toHaveBeenLastCalledWith(session.id, expect.objectContaining({ where: [{ column: "post_parent", value: null }] }));
    });

    it("requests the table without the column filter once the chip is cleared", async () => {
      useStore.getState().openRow("wp_posts", "ID", "7");
      render(<Active />);
      await screen.findByText("ID = 7");
      mocked.browseTable.mockResolvedValue({ result: resultSet(), hasMore: false, limit: 500, filter: null, columnsFilter: null });
      await userEvent.click(screen.getByRole("button", { name: "Clear column filter" }));
      await waitFor(() => expect(mocked.browseTable).toHaveBeenLastCalledWith(session.id, expect.objectContaining({ where: [] })));
      expect(screen.queryByText("ID = 7")).not.toBeInTheDocument();
    });
  });

  describe("paging", () => {
    const pageOf = (label: string, patch: Partial<BrowsePage> = {}): BrowsePage => ({
      result: resultSet({ rows: [[num("1"), text(`${label}-x`)], [num("2"), text(`${label}-y`)]] }),
      hasMore: true,
      limit: 2,
      filter: null,
      columnsFilter: null,
      ...patch,
    });
    const held = () => {
      let resolve!: (page: BrowsePage) => void;
      const promise = new Promise<BrowsePage>((r) => (resolve = r));
      return { promise, resolve };
    };

    it("shows the latest page when an earlier page's reply arrives last", async () => {
      const first = held();
      const second = held();
      mocked.browseTable.mockResolvedValueOnce(pageOf("p0")).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
      render(<TableTab tab={tab} />);
      await screen.findByText("p0-x");

      await userEvent.click(screen.getByRole("button", { name: "Next page" }));
      await userEvent.click(screen.getByRole("button", { name: "Next page" }));
      await act(async () => second.resolve(pageOf("p4", { hasMore: false })));
      await act(async () => first.resolve(pageOf("p2")));

      expect(await screen.findByText("p4-x")).toBeInTheDocument();
      expect(screen.queryByText("p2-x")).not.toBeInTheDocument();
      expect(screen.getByText("5–6")).toBeInTheDocument();
    });

    it("advances by the page size Rust served, not by the rowLimit setting", async () => {
      mocked.browseTable.mockResolvedValue(pageOf("p0", { limit: 500 }));
      render(<TableTab tab={tab} />);
      await screen.findByText("p0-x");
      await userEvent.click(screen.getByRole("button", { name: "Next page" }));
      await waitFor(() => expect(mocked.browseTable).toHaveBeenLastCalledWith(session.id, expect.objectContaining({ offset: 500 })));
    });
  });

  describe("editing", () => {
    it("reports an unavailable structure, opens no editor and disables Insert row", async () => {
      mocked.describeTable.mockRejectedValue({ kind: "database", message: "Table is locked", code: null });
      render(<TableTab tab={tab} />);
      expect(await screen.findByRole("alert")).toHaveTextContent(/Editing unavailable.*Table is locked/);
      await userEvent.dblClick(await screen.findByText("Hello world"));
      expect(screen.queryByLabelText("Edit cell")).not.toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Insert row" })).toBeDisabled();
    });

    it("offers Delete row when the primary key differs from the result column in case", async () => {
      mocked.describeTable.mockResolvedValue({
        ...structure,
        columns: [{ ...structure.columns[0], name: "id" }, structure.columns[1]],
        indexes: [{ name: "PRIMARY", unique: true, columns: ["id"] }],
      });
      render(<TableTab tab={tab} />);
      fireEvent.contextMenu(await screen.findByText("Hello world"));
      expect(screen.getByRole("menuitem", { name: "Delete row" })).toBeInTheDocument();
    });
  });

  describe("structure view", () => {
    const structureTab: TableTabState = { ...tab, view: "structure" };

    it("shows the DDL with a copy button when the table has a DDL", async () => {
      render(<TableTab tab={structureTab} />);
      expect(await screen.findByRole("heading", { name: /DDL/ })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Copy DDL" })).toBeInTheDocument();
      expect(screen.getByText("CREATE TABLE wp_posts (...)")).toBeInTheDocument();
    });

    it("hides the DDL heading and copy button when the DDL is null", async () => {
      mocked.describeTable.mockResolvedValue({ ...structure, ddl: null });
      render(<TableTab tab={structureTab} />);
      expect(await screen.findByRole("heading", { name: "Columns" })).toBeInTheDocument();
      expect(screen.queryByRole("heading", { name: /DDL/ })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Copy DDL" })).not.toBeInTheDocument();
    });
  });
});
