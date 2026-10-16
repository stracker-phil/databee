import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { connectedStore, num, resultSet, settings, text } from "../test/fixtures";
import { ResultsGrid } from "./ResultsGrid";

describe("ResultsGrid editing", () => {
  beforeEach(() => connectedStore());

  it("edits a cell on double-click and reports the new value on Enter", async () => {
    const rs = resultSet();
    const onEditCell = vi.fn();
    render(<ResultsGrid columns={rs.columns} rows={rs.rows} onEditCell={onEditCell} />);
    await userEvent.dblClick(screen.getByText("Hello world"));
    const input = screen.getByLabelText("Edit cell");
    await userEvent.clear(input);
    await userEvent.type(input, "Hi{Enter}");
    expect(onEditCell).toHaveBeenCalledWith(0, 1, "Hi");
  });

  it("offers NULL and row actions in the context menu", async () => {
    const rs = resultSet();
    const onEditCell = vi.fn();
    const remove = vi.fn();
    render(<ResultsGrid columns={rs.columns} rows={rs.rows} onEditCell={onEditCell} rowActions={(row) => [{ label: `Delete row ${row}`, action: remove }]} />);
    fireEvent.contextMenu(screen.getByText("Hello world"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Set to NULL" }));
    expect(onEditCell).toHaveBeenCalledWith(0, 1, null);
    fireEvent.contextMenu(screen.getByText("Hello world"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete row 0" }));
    expect(remove).toHaveBeenCalled();
  });

  it("is read-only without an edit handler", async () => {
    const rs = resultSet();
    render(<ResultsGrid columns={rs.columns} rows={rs.rows} />);
    await userEvent.dblClick(screen.getByText("Hello world"));
    expect(screen.queryByLabelText("Edit cell")).not.toBeInTheDocument();
  });

  it("opens the editor for a text cell on Enter", async () => {
    const rs = resultSet();
    render(<ResultsGrid columns={rs.columns} rows={rs.rows} onEditCell={vi.fn()} />);
    await userEvent.click(screen.getByText("Hello world"));
    await userEvent.keyboard("{Enter}");
    expect(screen.getByLabelText("Edit cell")).toHaveValue("Hello world");
  });

  describe("with a bytes cell", () => {
    const rs = resultSet({ rows: [[num("1"), { t: "bytes", v: { len: 2, hexPreview: "cafe" } }]] });
    const bytes = () => screen.getByText(/0xcafe/);

    it("opens no editor on double-click", async () => {
      render(<ResultsGrid columns={rs.columns} rows={rs.rows} onEditCell={vi.fn()} />);
      await userEvent.dblClick(bytes());
      expect(screen.queryByLabelText("Edit cell")).not.toBeInTheDocument();
    });

    it("opens no editor on Enter", async () => {
      render(<ResultsGrid columns={rs.columns} rows={rs.rows} onEditCell={vi.fn()} />);
      await userEvent.click(bytes());
      await userEvent.keyboard("{Enter}");
      expect(screen.queryByLabelText("Edit cell")).not.toBeInTheDocument();
    });

    it("disables Edit value in the context menu", () => {
      render(<ResultsGrid columns={rs.columns} rows={rs.rows} onEditCell={vi.fn()} />);
      fireEvent.contextMenu(bytes());
      expect(screen.getByRole("menuitem", { name: "Edit value" })).toBeDisabled();
    });
  });
});

describe("ResultsGrid keyboard selection", () => {
  const columns = resultSet().columns;
  const rows = [0, 1, 2].map((i) => [text(`a${i}`), text(`b${i}`)]);

  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
  });

  it.each([
    { key: "ArrowDown", from: "a0", to: "a1" },
    { key: "ArrowDown", from: "a2", to: "a2" },
    { key: "ArrowRight", from: "a1", to: "b1" },
    { key: "ArrowRight", from: "b0", to: "b0" },
    { key: "Home", from: "b1", to: "a1" },
    { key: "End", from: "a1", to: "b1" },
  ])("$key moves the selection from $from to $to", async ({ key, from, to }) => {
    render(<ResultsGrid columns={columns} rows={rows} />);
    await userEvent.click(screen.getByText(from));
    await userEvent.keyboard(`{${key}}`);
    expect(screen.getByText(to)).toHaveClass("selected");
    if (to !== from) expect(screen.getByText(from)).not.toHaveClass("selected");
  });

  it("drops the selection when the rows are replaced by fewer rows", async () => {
    const many = Array.from({ length: 8 }, (_, i) => [text(`a${i}`), text(`b${i}`)]);
    const { rerender } = render(<ResultsGrid columns={columns} rows={many} />);
    await userEvent.click(screen.getByText("b5"));
    expect(screen.getByText("b5")).toHaveClass("selected");

    rerender(<ResultsGrid columns={columns} rows={many.slice(0, 2)} />);
    await userEvent.keyboard("{Meta>}c{/Meta}");
    await userEvent.keyboard("{ArrowDown}{ArrowRight}");

    for (const cell of screen.getAllByRole("gridcell")) expect(cell).not.toHaveClass("selected");
    expect(navigator.clipboard.writeText).not.toHaveBeenCalled();
  });
});

describe("ResultsGrid sizing", () => {
  const rowTops = () =>
    screen
      .getAllByRole("row")
      .slice(1)
      .map((r) => parseFloat((r as HTMLElement).style.top));

  it("lays rows out without gaps or overlap at any data size", () => {
    const rows = Array.from({ length: 5 }, (_, i) => [num(String(i)), text(`row ${i}`)]);
    const rs = resultSet({ rows });
    connectedStore({ settings: settings({ dataFontSize: 12 }) });
    const { unmount } = render(<ResultsGrid columns={rs.columns} rows={rs.rows} />);
    const small = rowTops();
    expect(small.slice(1).map((top, i) => top - small[i])).toEqual([26, 26, 26, 26]);
    unmount();

    connectedStore({ settings: settings({ dataFontSize: 18 }) });
    render(<ResultsGrid columns={rs.columns} rows={rs.rows} />);
    const large = rowTops();
    const step = large[1] - large[0];
    expect(step).toBe(39);
    expect(large.slice(1).map((top, i) => top - large[i])).toEqual([step, step, step, step]);
    const row = screen.getAllByRole("row")[1] as HTMLElement;
    expect(parseFloat(row.style.height)).toBe(step);
  });
});
