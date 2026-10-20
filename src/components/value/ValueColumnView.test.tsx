import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import App from "../../App";
import { api, type BrowsePage, type Cell, type DecodedValue } from "../../api";
import { installCommands, runCommand } from "../../commands";
import type { ValueColumn } from "../../lib/columns";
import { useStore } from "../../state/store";
import { openOrder, stubDocuments } from "../../test/columnFixtures";
import { browsePage, columnMeta, connectedStore, num, resultSet, tableTab, text, value } from "../../test/fixtures";
import { activeObject } from "../../test/objectFixtures";

const mocked = vi.mocked(api, true);

const PHP = 'a:1:{s:3:"foo";s:3:"bar";}';
const EDITED = 'a:1:{s:3:"foo";s:3:"baz";}';
const RELOADED = 'a:1:{s:3:"foo";s:3:"qux";}';
const tab = tableTab({ table: "wp_postmeta" });

const treeOf = (leaf: string): DecodedValue => ({
  format: "php",
  root: { kind: "array", children: [{ key: "foo", keyKind: "string", node: { kind: "string", text: leaf } }] },
  warning: null,
  error: null,
});
const trees: Record<string, DecodedValue> = { [PHP]: treeOf("bar"), [EDITED]: treeOf("baz"), [RELOADED]: treeOf("qux") };

const columns = [columnMeta("ID", "BIGINT UNSIGNED"), columnMeta("meta_key", "VARCHAR"), columnMeta("meta_value", "LONGTEXT")];
const keys = [[{ column: "ID", value: "1" }]];

const page = browsePage({
  table: "wp_postmeta",
  result: resultSet({
    columns,
    rows: [[num("1"), text("_cfg"), text(PHP)]],
    totalRows: 1,
    serialized: [{ row: 0, col: 2, format: "php", preview: "a:1:{…}" }],
  }),
  keys,
});

const storing = (cell: Cell, id = 1): BrowsePage => browsePage({ table: "wp_postmeta", result: resultSet({ id, columns, rows: [[num("1"), text("_cfg"), cell]], totalRows: 1 }), keys });
const rowGone = browsePage({ table: "wp_postmeta", result: resultSet({ columns, rows: [], totalRows: 0 }), keys: [] });

const settle = () =>
  act(async () => {
    await new Promise((resolve) => setTimeout(resolve));
  });

/** What the by-key request of a reload answers; every other browse answers `page`. */
let stored: BrowsePage;

beforeEach(() => {
  vi.clearAllMocks();
  stored = page;
  mocked.browseTable.mockImplementation(async (_s, req) => (req.limit === 1 ? stored : page));
  mocked.decodeValue.mockImplementation(async (t) => trees[t] ?? null);
  mocked.fetchCell.mockResolvedValue("");
});

async function openValueColumn() {
  render(<App />);
  await userEvent.click(await screen.findByText("_cfg"));
  await userEvent.keyboard("{ArrowRight}{Enter}");
  return screen.findByRole("region", { name: "meta_value" });
}

describe("value column of a table tab", () => {
  beforeEach(() => {
    connectedStore({ tabs: [tab] });
    installCommands();
  });

  it("opens the selected serialized cell as a tree on Enter", async () => {
    await openValueColumn();
    expect(await screen.findByRole("tree", { name: "PHP value" })).toBeInTheDocument();
    expect(await screen.findByText("bar")).toBeInTheDocument();
    expect(mocked.decodeValue).toHaveBeenCalledWith(PHP);
  });

  it("offers no maximize button, as there are no panes to maximize against", async () => {
    await openValueColumn();
    await screen.findByRole("tree", { name: "PHP value" });
    expect(screen.queryByRole("button", { name: "Maximize pane" })).not.toBeInTheDocument();
  });

  it("queues the edited value as a setCell on the cell's row", async () => {
    mocked.editSerialized.mockResolvedValue(EDITED);
    await openValueColumn();
    await userEvent.click(await screen.findByRole("button", { name: "Edit foo" }));
    const input = screen.getByRole("textbox", { name: "New value" });
    await userEvent.clear(input);
    await userEvent.type(input, "baz{Enter}");

    expect(mocked.editSerialized).toHaveBeenCalledWith(PHP, [0], { kind: "string", text: "baz" });
    expect(mocked.planChange).toHaveBeenCalledWith("s1", { type: "setCell", database: "db", table: "wp_postmeta", keyColumn: "ID", key: "1", column: "meta_value", value: EDITED });
  });

  it("closes with its close button", async () => {
    await openValueColumn();
    await userEvent.click(screen.getByRole("button", { name: "Close value column" }));
    expect(screen.queryByRole("region", { name: "meta_value" })).not.toBeInTheDocument();
  });

  it("closes on Escape", async () => {
    await openValueColumn();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("region", { name: "meta_value" })).not.toBeInTheDocument();
  });

  describe("reading the value again", () => {
    it("shows the value stored now when Reload is clicked, read by the row's key", async () => {
      await openValueColumn();
      await screen.findByText("bar");
      stored = storing(text(RELOADED));
      await userEvent.click(screen.getByRole("button", { name: "Reload" }));

      expect(await screen.findByText("qux")).toBeInTheDocument();
      expect(screen.queryByText("bar")).not.toBeInTheDocument();
      expect(mocked.browseTable).toHaveBeenCalledWith("s1", { database: "db", table: "wp_postmeta", offset: 0, limit: 1, where: [{ column: "ID", value: "1" }] });
    });

    it("shows the value stored now on view.refresh", async () => {
      await openValueColumn();
      await screen.findByText("bar");
      stored = storing(text(RELOADED));
      await act(async () => {
        runCommand("view.refresh");
      });

      expect(await screen.findByText("qux")).toBeInTheDocument();
    });

    it("fetches the full text of a long text cell from the new result, at row 0", async () => {
      mocked.fetchCell.mockResolvedValue("long stored text, fetched in full");
      await openValueColumn();
      await screen.findByText("bar");
      stored = storing({ t: "longText", v: { len: 40000, preview: "long stored" } }, 7);
      await userEvent.click(screen.getByRole("button", { name: "Reload" }));

      expect(await screen.findByText("long stored text, fetched in full")).toBeInTheDocument();
      expect(mocked.fetchCell).toHaveBeenCalledWith("s1", 7, 0, 2);
    });

    it("says the row no longer exists when no row has the key, and offers nothing to copy or switch", async () => {
      await openValueColumn();
      await screen.findByText("bar");
      stored = rowGone;
      await userEvent.click(screen.getByRole("button", { name: "Reload" }));

      const message = await screen.findByText(/no longer exists/);
      expect(message).toHaveTextContent("Row wp_postmeta ID = 1 no longer exists");
      expect(screen.getByRole("button", { name: "Copy value" })).toBeDisabled();
      expect(screen.queryByRole("group", { name: "Show as" })).not.toBeInTheDocument();
      expect(screen.queryByRole("tree", { name: "PHP value" })).not.toBeInTheDocument();
    });

    it("toasts the error and keeps the old text when reading the value fails", async () => {
      await openValueColumn();
      await screen.findByText("bar");
      const locked = Object.assign(new Error("Table is locked"), { kind: "database", code: null });
      mocked.browseTable.mockImplementation((_s, req) => (req.limit === 1 ? Promise.reject(locked) : Promise.resolve(page)));
      await userEvent.click(screen.getByRole("button", { name: "Reload" }));

      expect(await screen.findByText(/Could not reload the value/)).toHaveTextContent("Could not reload the value: Table is locked");
      expect(screen.getByText("bar")).toBeInTheDocument();
    });
  });

  describe("an edit queued from the column", () => {
    async function queueEdit() {
      mocked.editSerialized.mockResolvedValue(EDITED);
      await openValueColumn();
      await userEvent.click(await screen.findByRole("button", { name: "Edit foo" }));
      const input = screen.getByRole("textbox", { name: "New value" });
      await userEvent.clear(input);
      await userEvent.type(input, "baz{Enter}");
      await screen.findByText("baz");
      stored = storing(text(RELOADED));
    }

    it.each([
      ["Reload is clicked", () => userEvent.click(screen.getByRole("button", { name: "Reload" }))],
      ["⌘R is pressed", () => userEvent.keyboard("{Meta>}r{/Meta}")],
    ])("stays while it is pending when %s, and gives way to the stored value once it is applied", async (_when, reload) => {
      await queueEdit();
      await reload();
      await settle();
      expect(screen.getByText("baz")).toBeInTheDocument();
      expect(screen.queryByText("qux")).not.toBeInTheDocument();

      await userEvent.click(screen.getByRole("button", { name: /Apply \(⌘S\)/ }));
      expect(await screen.findByText("qux")).toBeInTheDocument();
      expect(screen.queryByText("baz")).not.toBeInTheDocument();
    });
  });
});

describe("value column of an object tab", () => {
  const metaValue = value({
    raw: text(PHP),
    kind: { kind: "serialized", format: "php", preview: "a:1:{…}" },
    source: { table: "wp_postmeta", column: "meta_value", row: [{ column: "ID", value: "1" }] },
  });
  const column: ValueColumn = { kind: "value", value: metaValue, text: PHP, database: "db" };

  async function openStoredValue() {
    await openOrder();
    act(() => useStore.getState().openValueColumn(activeObject().id, 0, column));
    await screen.findByRole("region", { name: "meta_value" });
    await screen.findByText("bar");
  }

  const refresh = () =>
    act(async () => {
      runCommand("view.refresh");
    });

  beforeEach(() => {
    connectedStore({ tabs: [] });
    installCommands();
    stubDocuments(mocked);
    mocked.browseTable.mockImplementation(async () => stored);
  });

  it("shows the value stored now on view.refresh", async () => {
    await openStoredValue();
    stored = storing(text(RELOADED));
    await refresh();

    expect(await screen.findByText("qux")).toBeInTheDocument();
    expect(mocked.browseTable).toHaveBeenCalledWith("s1", expect.objectContaining({ table: "wp_postmeta", limit: 1, where: [{ column: "ID", value: "1" }] }));
  });

  it("copies the value with y, and does nothing once its row is gone", async () => {
    await openStoredValue();
    await userEvent.keyboard("y");
    await waitFor(() => expect(navigator.clipboard.writeText).toHaveBeenCalledWith(PHP));
    vi.mocked(navigator.clipboard.writeText).mockClear();

    stored = rowGone;
    await refresh();
    await screen.findByText(/no longer exists/);
    await userEvent.keyboard("y");
    await settle();

    expect(navigator.clipboard.writeText).not.toHaveBeenCalled();
  });
});
