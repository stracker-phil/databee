import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api, type TableStructure } from "../api";
import { useStore, type TableTab as TableTabState } from "../state/store";
import { connectedStore, resultSet, session } from "../test/fixtures";
import { TableTab } from "./TableTab";

const mocked = vi.mocked(api, true);

const structure: TableStructure = {
  columns: [
    { name: "ID", columnType: "bigint unsigned", nullable: false, default: null, key: "PRI", extra: "auto_increment", comment: "" },
    { name: "post_title", columnType: "text", nullable: false, default: null, key: "", extra: "", comment: "" },
  ],
  indexes: [{ name: "PRIMARY", unique: true, columns: ["ID"] }],
  ddl: "CREATE TABLE wp_posts (...)",
};

const tab: TableTabState = { id: "t1", kind: "table", sessionId: session.id, title: "wp_posts", database: "db", table: "wp_posts", view: "data" };

describe("TableTab", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
    useStore.setState({ tabs: [tab], activeTabId: tab.id });
    mocked.describeTable.mockResolvedValue(structure);
    mocked.browseTable.mockResolvedValue({ result: resultSet(), hasMore: false, filter: null });
  });

  it("sends the primary key with the filter and shows the ID interpretation as a chip", async () => {
    mocked.browseTable.mockResolvedValue({
      result: resultSet(),
      hasMore: false,
      filter: { kind: "ids", sql: "`ID` IN (3229, 3227)", label: "ID in (3229, 3227)" },
    });
    render(<TableTab tab={tab} />);
    const where = screen.getByLabelText("WHERE filter");
    await waitFor(() => expect(mocked.browseTable).toHaveBeenLastCalledWith(session.id, expect.objectContaining({ primaryKey: "ID" })));
    await userEvent.type(where, "3229, 3227{Enter}");
    await waitFor(() => expect(mocked.browseTable).toHaveBeenLastCalledWith(session.id, expect.objectContaining({ filter: "3229, 3227", primaryKey: "ID" })));
    const chip = await screen.findByRole("button", { name: /ID in \(3229, 3227\)/ });
    await userEvent.click(chip);
    expect(screen.getByText("`ID` IN (3229, 3227)")).toBeInTheDocument();
  });

  it("shows no chip for plain SQL filters", async () => {
    mocked.browseTable.mockResolvedValue({ result: resultSet(), hasMore: false, filter: { kind: "sql", sql: "post_type = 'page'", label: "post_type = 'page'" } });
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
    useStore.setState({ uiState: { [session.identity]: { tables: { "db.wp_posts": { widths: { ID: 300, post_title: 90 } } }, splitWidth: null } } });
    render(<TableTab tab={tab} />);
    await screen.findByText("Hello world");
    const grid = screen.getByRole("grid");
    expect(grid.style.getPropertyValue("--col-0")).toBe("300px");
    fireEvent.doubleClick(screen.getByRole("separator", { name: "Resize ID" }));
    const fitted = parseFloat(grid.style.getPropertyValue("--col-0"));
    expect(fitted).toBeLessThan(300);
    expect(fitted).toBeGreaterThanOrEqual(39);
  });
});
