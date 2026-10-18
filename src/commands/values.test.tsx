import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import App from "../App";
import { api, type TableStructure } from "../api";
import { address } from "../lib/address";
import { selectActiveTab, useStore } from "../state/store";
import { browsePage, columnMeta, connectedStore, num, resultSet, text } from "../test/fixtures";
import { installCommands } from "./index";

const mocked = vi.mocked(api, true);

const structure: TableStructure = {
  columns: [],
  indexes: [{ name: "PRIMARY", unique: true, columns: ["ID"] }],
  primaryKey: ["ID"],
  foreignKeys: [],
  ddl: null,
};

const usersTable = address("shop", "db", { kind: "table", table: "wp_users", filter: [] });

const page = browsePage({
  result: resultSet({
    columns: [columnMeta("ID", "BIGINT"), columnMeta("post_title", "TEXT"), columnMeta("post_status", "VARCHAR"), columnMeta("post_author", "BIGINT")],
    rows: [
      [num("7"), text("Hello world"), text("publish"), num("12")],
      [num("8"), text("Second"), text("draft"), num("12")],
    ],
    kinds: [
      { kind: "number" },
      { kind: "text" },
      { kind: "enum" },
      { kind: "reference", targets: [{ table: usersTable, column: "ID", key: true }], by: { kind: "preset", preset: "wordpress", id: "post-author" } },
    ],
  }),
  keys: [[{ column: "ID", value: "7" }], [{ column: "ID", value: "8" }]],
});

const row7 = { table: "wp_posts", column: "post_title", row: [{ column: "ID", value: "7" }] };

const writtenToClipboard = () => vi.mocked(navigator.clipboard.writeText);

async function openPostsAndSelect(content: string) {
  render(<App />);
  await screen.findByText("wp_options");
  act(() => useStore.getState().openTableTab("db", "wp_posts"));
  await userEvent.click(await screen.findByText(content));
}

describe("value commands", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [] });
    mocked.describeTable.mockResolvedValue(structure);
    mocked.browseTable.mockResolvedValue(page);
    installCommands();
  });

  describe("y", () => {
    it("copies the selected value", async () => {
      await openPostsAndSelect("Hello world");
      await userEvent.keyboard("y");
      await waitFor(() => expect(writtenToClipboard()).toHaveBeenCalledWith("Hello world"));
    });

    it("copies the full text of a long value fetched from Rust", async () => {
      const long = "full text ".repeat(500);
      mocked.fetchCell.mockResolvedValue(long);
      mocked.browseTable.mockResolvedValue({
        ...page,
        result: { ...page.result, rows: [[num("7"), { t: "longText", v: { len: long.length, preview: "full text" } }, text("publish"), num("12")]] },
        keys: [page.keys[0]],
      });
      await openPostsAndSelect("full text… (5,000 chars)");
      await userEvent.keyboard("y");
      await waitFor(() => expect(writtenToClipboard()).toHaveBeenCalledWith(long));
    });
  });

  describe("Shift+Y", () => {
    it("copies the SQL condition Rust builds for the selected value", async () => {
      mocked.valueCondition.mockResolvedValue("`post_title` = 'Hello world'");
      await openPostsAndSelect("Hello world");
      await userEvent.keyboard("{Shift>}Y{/Shift}");
      await waitFor(() => expect(writtenToClipboard()).toHaveBeenCalledWith("`post_title` = 'Hello world'"));
      expect(mocked.valueCondition).toHaveBeenCalledWith("s1", "db", row7, "Hello world");
    });

    it("copies nothing and toasts when Rust cannot build the condition", async () => {
      mocked.valueCondition.mockRejectedValue({ kind: "invalid_input", message: "No key", code: null });
      await openPostsAndSelect("Hello world");
      await userEvent.keyboard("{Shift>}Y{/Shift}");
      await waitFor(() => expect(useStore.getState().toasts.at(-1)?.message).toContain("Could not build the condition: No key"));
      expect(writtenToClipboard()).not.toHaveBeenCalled();
    });
  });

  describe("f", () => {
    it("shows the rows that have the selected value", async () => {
      await openPostsAndSelect("Hello world");
      await userEvent.keyboard("f");
      await waitFor(() =>
        expect(mocked.browseTable).toHaveBeenLastCalledWith("s1", expect.objectContaining({ table: "wp_posts", where: [{ column: "post_title", value: "Hello world" }] })),
      );
    });

    it("filters by an enum value on click", async () => {
      render(<App />);
      await screen.findByText("wp_options");
      act(() => useStore.getState().openTableTab("db", "wp_posts"));
      await userEvent.click(await screen.findByRole("button", { name: "draft" }));
      await waitFor(() =>
        expect(mocked.browseTable).toHaveBeenLastCalledWith("s1", expect.objectContaining({ where: [{ column: "post_status", value: "draft" }] })),
      );
    });
  });

  describe("e", () => {
    it("edits the selected cell and queues the change on Enter", async () => {
      await openPostsAndSelect("Hello world");
      await userEvent.keyboard("e");
      const input = await screen.findByLabelText("Edit cell");
      expect(input).toHaveValue("Hello world");
      await userEvent.clear(input);
      await userEvent.type(input, "Hi{Enter}");
      await waitFor(() =>
        expect(mocked.planChange).toHaveBeenCalledWith("s1", {
          type: "setCell",
          database: "db",
          table: "wp_posts",
          keyColumn: "ID",
          key: "7",
          column: "post_title",
          value: "Hi",
        }),
      );
    });
  });

  it("leaves letters to the WHERE filter while it has focus", async () => {
    await openPostsAndSelect("Hello world");
    const where = screen.getByLabelText("WHERE filter");
    await userEvent.click(where);
    expect(useStore.getState().selectedCell).not.toBeNull();
    await userEvent.keyboard("yef");
    expect(where).toHaveValue("yef");
    expect(writtenToClipboard()).not.toHaveBeenCalled();
    expect(screen.queryByLabelText("Edit cell")).not.toBeInTheDocument();
  });

  it("does nothing without a selected value", async () => {
    render(<App />);
    await screen.findByText("wp_options");
    act(() => useStore.getState().openTableTab("db", "wp_posts"));
    await screen.findByText("Hello world");
    await userEvent.keyboard("y");
    expect(writtenToClipboard()).not.toHaveBeenCalled();
  });

  it("moves the table tab in place to the target of a reference on click", async () => {
    render(<App />);
    await screen.findByText("wp_options");
    act(() => useStore.getState().openTableTab("db", "wp_posts"));
    const before = selectActiveTab(useStore.getState())!;
    const tabCount = useStore.getState().tabs.length;
    const links = await screen.findAllByRole("link", { name: "12" });
    await userEvent.click(links[0]);
    await waitFor(() =>
      expect(selectActiveTab(useStore.getState())).toMatchObject({
        id: before.id,
        kind: "object",
        address: { target: { kind: "row", table: "wp_users", key: "12" } },
        nav: { back: [{ kind: "table" }] },
      }),
    );
    expect(useStore.getState().tabs).toHaveLength(tabCount);
  });
});
