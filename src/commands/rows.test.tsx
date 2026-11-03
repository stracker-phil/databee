import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import App from "../App";
import { api, type TableStructure } from "../api";
import { useStore } from "../state/store";
import { browsePage, columnMeta, connectedStore, num, queryTab, resultSet, session, text } from "../test/fixtures";
import { installCommands, runCommand } from "./index";

const mocked = vi.mocked(api, true);

const structure: TableStructure = {
  columns: [],
  indexes: [{ name: "PRIMARY", unique: true, columns: ["ID"] }],
  primaryKey: ["ID"],
  foreignKeys: [],
  ddl: null,
};

const page = browsePage({
  result: resultSet({
    columns: [columnMeta("ID", "BIGINT"), columnMeta("post_title", "TEXT")],
    rows: [
      [num("7"), text("Hello world")],
      [num("8"), text("Second")],
    ],
  }),
  keys: [[{ column: "ID", value: "7" }], [{ column: "ID", value: "8" }]],
});

const row7 = { database: "db", table: "wp_posts", keyColumn: "ID", key: "7" };
const rowOf = (content: string) => screen.getByText(content).closest<HTMLElement>('[role="row"]')!;

async function openPosts() {
  render(<App />);
  await screen.findByText("wp_options");
  act(() => useStore.getState().openTableTab("db", "wp_posts"));
  await screen.findByText("Hello world");
}

async function selectHello() {
  await openPosts();
  await userEvent.click(screen.getByText("Hello world"));
}

describe("row commands", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [] });
    mocked.describeTable.mockResolvedValue(structure);
    mocked.browseTable.mockResolvedValue(page);
    installCommands();
  });

  describe("⌘⌫", () => {
    it("queues the deletion of the selected row and marks it", async () => {
      await selectHello();
      await userEvent.keyboard("{Meta>}{Backspace}{/Meta}");
      await waitFor(() => expect(rowOf("Hello world")).toHaveClass("pending-delete"));
      expect(mocked.planChange).toHaveBeenCalledWith(session.id, { type: "deleteRow", ...row7 });
      expect(rowOf("Second")).not.toHaveClass("pending-delete");
    });

    it("discards the plan when the row is already pending deletion", async () => {
      await selectHello();
      await userEvent.keyboard("{Meta>}{Backspace}{/Meta}");
      await waitFor(() => expect(rowOf("Hello world")).toHaveClass("pending-delete"));
      await userEvent.keyboard("{Meta>}{Backspace}{/Meta}");
      await waitFor(() => expect(rowOf("Hello world")).not.toHaveClass("pending-delete"));
      expect(mocked.planChange).toHaveBeenCalledTimes(1);
    });

    it("queues nothing while the focus is in the WHERE filter", async () => {
      await selectHello();
      await userEvent.click(screen.getByLabelText("WHERE filter"));
      await userEvent.keyboard("{Meta>}{Backspace}{/Meta}");
      expect(mocked.planChange).not.toHaveBeenCalled();
    });
  });

  it("queues a duplicate of the selected row on ⌘D", async () => {
    await selectHello();
    await userEvent.keyboard("{Meta>}d{/Meta}");
    await waitFor(() => expect(mocked.planChange).toHaveBeenCalledWith(session.id, { type: "duplicateRow", ...row7 }));
  });

  it("queues NULL for the selected column on ⌘⇧⌫", async () => {
    await selectHello();
    await userEvent.keyboard("{Meta>}{Shift>}{Backspace}{/Shift}{/Meta}");
    await waitFor(() => expect(mocked.planChange).toHaveBeenCalledWith(session.id, { type: "setCell", ...row7, column: "post_title", value: null }));
  });

  describe("availability", () => {
    it.each(["row.delete", "row.duplicate", "value.setNull", "row.openObject"])("does not run %s without a selected cell", async (id) => {
      await openPosts();
      expect(runCommand(id)).toBe(false);
      expect(mocked.planChange).not.toHaveBeenCalled();
    });

    it.each(["row.delete", "row.duplicate", "value.setNull", "row.openObject"])("does not run %s in a query tab", (id) => {
      connectedStore({ tabs: [queryTab()] });
      expect(runCommand(id)).toBe(false);
    });

    it("runs the commands once a cell is selected", async () => {
      await selectHello();
      expect(runCommand("row.duplicate")).toBe(true);
      await waitFor(() => expect(mocked.planChange).toHaveBeenCalledWith(session.id, { type: "duplicateRow", ...row7 }));
    });
  });

  it("opens the selected row as an object", async () => {
    await selectHello();
    runCommand("row.openObject");
    expect(useStore.getState().tabs.at(-1)).toMatchObject({ kind: "object", address: { target: { kind: "row", table: "wp_posts", key: "7" } } });
  });

  describe("copying", () => {
    const written = () => vi.mocked(navigator.clipboard.writeText);

    it("copies the selected row as TSV with the column names", async () => {
      await selectHello();
      runCommand("row.copyTsv");
      await waitFor(() => expect(written()).toHaveBeenCalledWith("ID\tpost_title\n7\tHello world"));
    });

    it("copies the selected row as JSON", async () => {
      await selectHello();
      runCommand("row.copyJson");
      await waitFor(() => expect(written()).toHaveBeenCalledWith(JSON.stringify([{ ID: "7", post_title: "Hello world" }], null, 2)));
    });

    it("copies every row of the grid as TSV", async () => {
      await selectHello();
      runCommand("grid.copyAllTsv");
      await waitFor(() => expect(written()).toHaveBeenCalledWith("ID\tpost_title\n7\tHello world\n8\tSecond"));
    });

    it("copies nothing without a selected cell", async () => {
      await openPosts();
      expect(runCommand("row.copyTsv")).toBe(false);
      expect(written()).not.toHaveBeenCalled();
    });
  });

  describe("surfaces", () => {
    it("lists Delete row with its key in the command palette for a selected row", async () => {
      await selectHello();
      await userEvent.keyboard("{Meta>}k{/Meta}");
      await userEvent.type(await screen.findByLabelText("Search everything"), ">");
      const option = await screen.findByRole("option", { name: /Delete row/ });
      expect(option).toHaveTextContent("⌘⌫");
    });

    it("leaves Delete row out of the command palette without a selection", async () => {
      await openPosts();
      await userEvent.keyboard("{Meta>}k{/Meta}");
      await userEvent.type(await screen.findByLabelText("Search everything"), ">Delete row");
      expect(screen.queryByRole("option", { name: /Delete row/ })).not.toBeInTheDocument();
    });

    it("marks the row pending deletion from the right-click menu, which shows the key", async () => {
      await openPosts();
      fireEvent.contextMenu(screen.getByText("Hello world"));
      const item = await screen.findByRole("menuitem", { name: "Delete row" });
      expect(within(item).getByText("⌘⌫")).toBeInTheDocument();
      await userEvent.click(item);
      await waitFor(() => expect(rowOf("Hello world")).toHaveClass("pending-delete"));
      expect(mocked.planChange).toHaveBeenCalledWith(session.id, { type: "deleteRow", ...row7 });
    });
  });
});
