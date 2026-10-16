import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api, type Cell, type DecodedValue } from "../api";
import { useStore } from "../state/store";
import { connectedStore, text } from "../test/fixtures";
import { Inspector } from "./Inspector";

const mocked = vi.mocked(api, true);

const column = { name: "meta_value", typeName: "LONGTEXT" };

const jsonOf = (key: string): DecodedValue => ({
  format: "json",
  root: { kind: "object", children: [{ key, keyKind: "string", node: { kind: "int", text: "1" } }] },
  warning: null,
  error: null,
});

const select = (row: number, cell: Cell) => act(() => useStore.getState().selectCell({ row, col: 0, column, cell }));

describe("Inspector", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
    mocked.decodeValue.mockImplementation(async (value: string) => (value.startsWith("{") ? jsonOf(value.includes("first") ? "first" : "second") : null));
  });

  it("renders nothing without a selected cell", () => {
    const { container } = render(<Inspector />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows plain text as is with its length", async () => {
    render(<Inspector />);
    select(0, text("hello"));
    expect(await screen.findByText("hello")).toBeInTheDocument();
    expect(screen.getByText("5 characters")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Raw" })).not.toBeInTheDocument();
  });

  it("shows the decoded tree and switches to the raw text on Raw", async () => {
    render(<Inspector />);
    select(0, text('{"first":1}'));
    expect(await screen.findByRole("tree", { name: "JSON value" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Raw" }));
    expect(screen.queryByRole("tree")).not.toBeInTheDocument();
    expect(screen.getByText('{"first":1}')).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Raw", pressed: true })).toBeInTheDocument();
  });

  it("returns to the decoded view when another cell is selected after Raw", async () => {
    render(<Inspector />);
    select(0, text('{"first":1}'));
    await userEvent.click(await screen.findByRole("button", { name: "Raw" }));
    expect(screen.getByRole("button", { name: "Raw", pressed: true })).toBeInTheDocument();

    select(1, text('{"second":2}'));
    expect(await screen.findByRole("tree", { name: "JSON value" })).toBeInTheDocument();
    expect(screen.getByText("second")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Raw", pressed: false })).toBeInTheDocument();
  });

  it("shows the configured NULL label for a NULL cell", async () => {
    render(<Inspector />);
    select(0, { t: "null" });
    expect(await screen.findByText("NULL")).toBeInTheDocument();
  });
});
