import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api } from "../api";
import { useStore } from "../state/store";
import { connectedStore, session } from "../test/fixtures";
import { Sidebar } from "./Sidebar";

const mocked = vi.mocked(api, true);

describe("Sidebar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
  });

  it("lists tables of the current database and hides system databases", () => {
    render(<Sidebar />);
    const tree = screen.getByRole("tree");
    expect(within(tree).getByText("wp_options")).toBeInTheDocument();
    expect(within(tree).queryByText("mysql")).not.toBeInTheDocument();
  });

  it("filters tables by name", async () => {
    render(<Sidebar />);
    await userEvent.type(screen.getByLabelText("Filter tables"), "post");
    expect(screen.getByText("wp_posts")).toBeInTheDocument();
    expect(screen.getByText("wp_postmeta")).toBeInTheDocument();
    expect(screen.queryByText("wp_options")).not.toBeInTheDocument();
  });

  it("opens a table tab on click and reuses it on the next click", async () => {
    render(<Sidebar />);
    await userEvent.click(screen.getByText("wp_options"));
    await userEvent.click(screen.getByText("wp_options"));
    const tables = useStore.getState().tabs.filter((t) => t.kind === "table");
    expect(tables).toHaveLength(1);
    expect(useStore.getState().activeTabId).toBe(tables[0].id);
  });

  it("reuses one preview tab for single clicks and keeps pinned tabs", async () => {
    render(<Sidebar />);
    await userEvent.click(screen.getByText("wp_options"));
    await userEvent.click(screen.getByText("wp_posts"));
    let tables = useStore.getState().tabs.filter((t) => t.kind === "table");
    expect(tables.map((t) => t.kind === "table" && t.table)).toEqual(["wp_posts"]);
    expect(tables[0].kind === "table" && tables[0].preview).toBe(true);

    await userEvent.dblClick(screen.getByText("wp_posts"));
    await userEvent.click(screen.getByText("wp_options"));
    tables = useStore.getState().tabs.filter((t) => t.kind === "table");
    expect(tables.map((t) => t.kind === "table" && [t.table, !!t.preview])).toEqual([
      ["wp_posts", false],
      ["wp_options", true],
    ]);

    fireEvent.click(screen.getByText("wp_postmeta"), { metaKey: true });
    tables = useStore.getState().tabs.filter((t) => t.kind === "table");
    expect(tables.map((t) => t.kind === "table" && [t.table, !!t.preview])).toEqual([
      ["wp_posts", false],
      ["wp_options", true],
      ["wp_postmeta", false],
    ]);
  });

  it("shows query history for the server and reopens an entry", async () => {
    mocked.listHistory.mockResolvedValue([
      { account: "root@127.0.0.1:52958", database: "db", sql: "SELECT 42", truncated: false, executedAt: new Date().toISOString(), elapsedMs: 3, error: null },
    ]);
    render(<Sidebar />);
    await userEvent.click(screen.getByRole("button", { name: /History/ }));
    const entry = await screen.findByText("SELECT 42");
    expect(mocked.listHistory).toHaveBeenCalledWith("root@127.0.0.1:52958");
    await userEvent.dblClick(entry);
    const opened = useStore.getState().tabs.at(-1)!;
    expect(opened.kind === "query" && opened.sql).toBe("SELECT 42");
    expect(opened.sessionId).toBe(session.id);
  });
});
