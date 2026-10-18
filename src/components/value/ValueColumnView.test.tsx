import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import App from "../../App";
import { api, type DecodedValue } from "../../api";
import { installCommands } from "../../commands";
import { browsePage, columnMeta, connectedStore, num, resultSet, tableTab, text } from "../../test/fixtures";

const mocked = vi.mocked(api, true);

const PHP = 'a:1:{s:3:"foo";s:3:"bar";}';
const EDITED = 'a:1:{s:3:"foo";s:3:"baz";}';
const tab = tableTab({ table: "wp_postmeta" });

const decoded: DecodedValue = {
  format: "php",
  root: { kind: "array", children: [{ key: "foo", keyKind: "string", node: { kind: "string", text: "bar" } }] },
  warning: null,
  error: null,
};

const page = browsePage({
  table: "wp_postmeta",
  result: resultSet({
    columns: [columnMeta("ID", "BIGINT UNSIGNED"), columnMeta("meta_key", "VARCHAR"), columnMeta("meta_value", "LONGTEXT")],
    rows: [[num("1"), text("_cfg"), text(PHP)]],
    totalRows: 1,
    serialized: [{ row: 0, col: 2, format: "php", preview: "a:1:{…}" }],
  }),
  keys: [[{ column: "ID", value: "1" }]],
});

async function openValueColumn() {
  render(<App />);
  await userEvent.click(await screen.findByText("_cfg"));
  await userEvent.keyboard("{ArrowRight}{Enter}");
  return screen.findByRole("region", { name: "meta_value" });
}

describe("value column of a table tab", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [tab] });
    installCommands();
    mocked.browseTable.mockResolvedValue(page);
    mocked.decodeValue.mockResolvedValue(decoded);
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
});
