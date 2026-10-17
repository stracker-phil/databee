import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useRef, type ComponentProps } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api, type Cell } from "../api";
import { address } from "../lib/address";
import { syncRowHeight } from "../lib/theme";
import { useStore } from "../state/store";
import { connectedStore, num, resultSet, settings, text, value } from "../test/fixtures";
import { ResultsGrid } from "./ResultsGrid";
import { useValueHover } from "./value/useValueHover";

const mocked = vi.mocked(api, true);
const source = { sessionId: "s1", resultId: 1 };

describe("ResultsGrid editing", () => {
  beforeEach(() => connectedStore());

  it("edits a cell on double-click and reports the new value on Enter", async () => {
    const rs = resultSet();
    const onEditCell = vi.fn();
    render(<ResultsGrid columns={rs.columns} rows={rs.rows} source={source} onEditCell={onEditCell} />);
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
    render(<ResultsGrid columns={rs.columns} rows={rs.rows} source={source} onEditCell={onEditCell} rowActions={(row) => [{ label: `Delete row ${row}`, action: remove }]} />);
    fireEvent.contextMenu(screen.getByText("Hello world"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Set to NULL" }));
    expect(onEditCell).toHaveBeenCalledWith(0, 1, null);
    fireEvent.contextMenu(screen.getByText("Hello world"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete row 0" }));
    expect(remove).toHaveBeenCalled();
  });

  it("is read-only without an edit handler", async () => {
    const rs = resultSet();
    render(<ResultsGrid columns={rs.columns} rows={rs.rows} source={source} />);
    await userEvent.dblClick(screen.getByText("Hello world"));
    expect(screen.queryByLabelText("Edit cell")).not.toBeInTheDocument();
  });

  it("opens the editor for a text cell on Enter", async () => {
    const rs = resultSet();
    render(<ResultsGrid columns={rs.columns} rows={rs.rows} source={source} onEditCell={vi.fn()} />);
    await userEvent.click(screen.getByText("Hello world"));
    await userEvent.keyboard("{Enter}");
    expect(screen.getByLabelText("Edit cell")).toHaveValue("Hello world");
  });

  describe("with a bytes cell", () => {
    const rs = resultSet({ rows: [[num("1"), { t: "bytes", v: { len: 2, hexPreview: "cafe" } }]] });
    const bytes = () => screen.getByText("<2 bytes>");

    it("opens no editor on double-click", async () => {
      render(<ResultsGrid columns={rs.columns} rows={rs.rows} source={source} onEditCell={vi.fn()} />);
      await userEvent.dblClick(bytes());
      expect(screen.queryByLabelText("Edit cell")).not.toBeInTheDocument();
    });

    it("opens no editor on Enter", async () => {
      render(<ResultsGrid columns={rs.columns} rows={rs.rows} source={source} onEditCell={vi.fn()} />);
      await userEvent.click(bytes());
      await userEvent.keyboard("{Enter}");
      expect(screen.queryByLabelText("Edit cell")).not.toBeInTheDocument();
    });

    it("disables Edit value in the context menu", () => {
      render(<ResultsGrid columns={rs.columns} rows={rs.rows} source={source} onEditCell={vi.fn()} />);
      fireEvent.contextMenu(bytes());
      expect(screen.getByRole("menuitem", { name: "Edit value" })).toBeDisabled();
    });
  });
});

function HoverGrid(props: Omit<ComponentProps<typeof ResultsGrid>, "source">) {
  const ref = useRef<HTMLDivElement>(null);
  useValueHover(ref);
  return (
    <div ref={ref}>
      <ResultsGrid {...props} source={source} />
    </div>
  );
}

describe("ResultsGrid row hover", () => {
  const provenance = (table: string, id: string) => ({ table, column: "c", row: [{ column: "ID", value: id }] });
  const rs = resultSet();
  const rowOf = (cell: string) => screen.getByText(cell).closest<HTMLElement>('[role="row"]')!;

  beforeEach(() => connectedStore());

  it("links the table of a row whose values are all stored in one table", async () => {
    const valueAt = (row: number, col: number) => value({ raw: rs.rows[row][col], source: provenance("wp_posts", "1") });
    render(<HoverGrid columns={rs.columns} rows={rs.rows} valueAt={valueAt} />);
    await userEvent.hover(rowOf("Hello world"));
    await waitFor(() => expect(useStore.getState().hover).toEqual({ src: null, table: "wp_posts", linked: false }));
  });

  it("links no table for a row mixing values of several tables", async () => {
    const valueAt = (row: number, col: number) => value({ raw: rs.rows[row][col], source: provenance(col === 0 ? "wp_posts" : "wp_users", "1") });
    render(<HoverGrid columns={rs.columns} rows={rs.rows} valueAt={valueAt} />);
    await userEvent.hover(screen.getByText("Hello world"));
    await waitFor(() => expect(useStore.getState().hover.table).toBe("wp_users"));
    await userEvent.hover(rowOf("Hello world"));
    await waitFor(() => expect(useStore.getState().hover.table).toBeNull());
  });
});

describe("ResultsGrid primary key cells", () => {
  const rs = resultSet();
  const rows = [rs.rows[0]];
  const valueAt = (row: number, col: number) =>
    col === 0
      ? value({ raw: rows[row][col], kind: { kind: "primaryKey", address: address("shop", "db", { kind: "row", table: "wp_posts", key: "1" }) } })
      : value({ raw: rows[row][col] });
  const lastTab = () => useStore.getState().tabs.at(-1);

  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
  });

  it("opens the row as a pinned object on double-click instead of editing", async () => {
    render(<ResultsGrid columns={rs.columns} rows={rows} source={source} valueAt={valueAt} onEditCell={vi.fn()} />);
    await userEvent.dblClick(screen.getByText("1"));
    expect(lastTab()).toMatchObject({ kind: "object", preview: false, address: { target: { kind: "row", table: "wp_posts", key: "1" } } });
    expect(screen.queryByLabelText("Edit cell")).not.toBeInTheDocument();
  });

  it("opens the row in the preview tab on Enter instead of editing", async () => {
    render(<ResultsGrid columns={rs.columns} rows={rows} source={source} valueAt={valueAt} onEditCell={vi.fn()} />);
    await userEvent.click(screen.getByText("Hello world"));
    await userEvent.keyboard("{Home}{Enter}");
    expect(lastTab()).toMatchObject({ kind: "object", preview: true, address: { target: { kind: "row", table: "wp_posts", key: "1" } } });
    expect(screen.queryByLabelText("Edit cell")).not.toBeInTheDocument();
  });

  it("still edits the other cells of the row on double-click", async () => {
    render(<ResultsGrid columns={rs.columns} rows={rows} source={source} valueAt={valueAt} onEditCell={vi.fn()} />);
    await userEvent.dblClick(screen.getByText("Hello world"));
    expect(screen.getByLabelText("Edit cell")).toHaveValue("Hello world");
  });
});

describe("ResultsGrid long text cells", () => {
  const columns = resultSet().columns;
  const rows = [[num("1"), { t: "longText", v: { len: 12000, preview: "start of it" } } as Cell]];
  const source = { sessionId: "s1", resultId: 4 };
  const full = "full value ".repeat(1000);
  const preview = () => screen.getByText(/start of it/);

  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
    mocked.fetchCell.mockResolvedValue(full);
  });

  it("shows the preview with the character count", () => {
    render(<ResultsGrid columns={columns} rows={rows} source={source} />);
    expect(preview()).toHaveTextContent("start of it… (12,000 chars)");
  });

  it("copies the full value fetched for the cell", async () => {
    render(<ResultsGrid columns={columns} rows={rows} source={source} />);
    await userEvent.click(preview());
    await userEvent.keyboard("{Meta>}c{/Meta}");
    await waitFor(() => expect(navigator.clipboard.writeText).toHaveBeenCalledWith(full));
    expect(mocked.fetchCell).toHaveBeenCalledWith("s1", 4, 0, 1);
  });

  it("pre-fills the editor with the full value, not the preview", async () => {
    render(<ResultsGrid columns={columns} rows={rows} source={source} onEditCell={vi.fn()} />);
    await userEvent.dblClick(preview());
    expect(await screen.findByLabelText("Edit cell")).toHaveValue(full);
  });

  it("opens no editor and toasts when the full value cannot be fetched", async () => {
    mocked.fetchCell.mockRejectedValue({ kind: "invalid_input", message: "Result expired; run the query again", code: null });
    render(<ResultsGrid columns={columns} rows={rows} source={source} onEditCell={vi.fn()} />);
    await userEvent.dblClick(preview());
    await waitFor(() => expect(useStore.getState().toasts.at(-1)?.message).toContain("Result expired; run the query again"));
    expect(screen.queryByLabelText("Edit cell")).not.toBeInTheDocument();
  });

});

const cellOf = (content: string) => screen.getByText(content).closest<HTMLElement>('[role="gridcell"]')!;

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
    render(<ResultsGrid columns={columns} rows={rows} source={source} />);
    await userEvent.click(screen.getByText(from));
    await userEvent.keyboard(`{${key}}`);
    expect(cellOf(to)).toHaveClass("selected");
    if (to !== from) expect(cellOf(from)).not.toHaveClass("selected");
  });

  it("drops the selection when the rows are replaced by fewer rows", async () => {
    const many = Array.from({ length: 8 }, (_, i) => [text(`a${i}`), text(`b${i}`)]);
    const { rerender } = render(<ResultsGrid columns={columns} rows={many} source={source} />);
    await userEvent.click(screen.getByText("b5"));
    expect(cellOf("b5")).toHaveClass("selected");

    rerender(<ResultsGrid columns={columns} rows={many.slice(0, 2)} source={source} />);
    await userEvent.keyboard("{Meta>}c{/Meta}");
    await userEvent.keyboard("{ArrowDown}{ArrowRight}");

    for (const cell of screen.getAllByRole("gridcell")) expect(cell).not.toHaveClass("selected");
    expect(navigator.clipboard.writeText).not.toHaveBeenCalled();
  });
});

describe("ResultsGrid value grammar", () => {
  const rs = resultSet();
  const rows = [rs.rows[0], rs.rows[1]];

  beforeEach(() => connectedStore());

  it("renders no row-number cell: a row has one cell per column", () => {
    render(<ResultsGrid columns={rs.columns} rows={[rs.rows[0]]} source={source} />);
    expect(screen.getAllByRole("gridcell")).toHaveLength(rs.columns.length);
  });

  it("right-aligns number cells, draws NULL by its kind and adds no per-type classes", () => {
    render(<ResultsGrid columns={rs.columns} rows={rows} source={source} />);
    expect(cellOf("1")).toHaveClass("num");
    expect(screen.getByText("NULL")).toHaveClass("val-null");
    for (const cell of screen.getAllByRole("gridcell")) expect(cell.className).not.toMatch(/(^|\s)t-/);
  });

  it("shows each column's type under its name", () => {
    render(<ResultsGrid columns={rs.columns} rows={rows} source={source} />);
    expect(screen.getByText("bigint unsigned")).toBeInTheDocument();
    expect(screen.getByText("text")).toBeInTheDocument();
  });
});

describe("ResultsGrid sizing", () => {
  const rowTops = () =>
    screen
      .getAllByRole("row")
      .slice(1)
      .map((r) => parseFloat((r).style.top));

  const measuredRowHeight = (height: number) => {
    const original = Element.prototype.getBoundingClientRect;
    return vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
      return (this as HTMLElement).style.height.includes("--row-h") ? ({ height } as DOMRect) : original.call(this);
    });
  };

  it("lays rows out without gaps or overlap at the default and a measured row height", () => {
    const rows = Array.from({ length: 5 }, (_, i) => [num(String(i)), text(`row ${i}`)]);
    const rs = resultSet({ rows });
    connectedStore({ settings: settings() });
    const { unmount } = render(<ResultsGrid columns={rs.columns} rows={rs.rows} source={source} />);
    const small = rowTops();
    expect(small.slice(1).map((top, i) => top - small[i])).toEqual([26, 26, 26, 26]);
    unmount();

    const spy = measuredRowHeight(39);
    try {
      act(() => syncRowHeight());
      render(<ResultsGrid columns={rs.columns} rows={rs.rows} source={source} />);
      const large = rowTops();
      expect(large.slice(1).map((top, i) => top - large[i])).toEqual([39, 39, 39, 39]);
      const row = screen.getAllByRole("row")[1];
      expect(parseFloat(row.style.height)).toBe(39);
    } finally {
      spy.mockRestore();
      act(() => syncRowHeight());
    }
  });
});
