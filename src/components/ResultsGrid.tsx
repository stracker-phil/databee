import { useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp } from "lucide-react";
import type { Address, Cell, ColumnMeta, Value as StoredValue } from "../api";
import type { CellSource } from "../lib/cells";
import { copyText } from "../lib/clipboard";
import { useRowHeight } from "../lib/theme";
import { useStore } from "../state/store";
import { useContextMenu, type MenuEntry } from "./ContextMenu";
import { cellMenu } from "./grid/cellMenu";
import { useCellEditing } from "./grid/useCellEditing";
import { useColumnSizing } from "./grid/useColumnSizing";
import { markScrolled, useEditValueCommand, useFocusCell, useRowValues, type CellPos } from "./gridHooks";
import { openTarget, showValue } from "./value/open";
import { useOpenValue } from "./value/useOpenValue";
import { hasLeadIcon, LEAD_ICON_CH, Value } from "./value/Value";

export type { CellPos } from "./gridHooks";

interface Props {
  columns: ColumnMeta[];
  rows: Cell[][];
  /** The result set the rows belong to; without it `longText` cells can't be fetched in full. */
  source: CellSource;
  /** A cell as a value (kind, provenance, style); without it cells show only what the cell itself says. */
  valueAt?: (row: number, col: number) => StoredValue;
  sort?: { column: string | null; descending: boolean };
  onSort?: (column: string) => void;
  /** Extra context-menu entries for a row (e.g. "Open as document"). */
  rowActions?: (row: number) => MenuEntry[];
  /** Enables double-click editing; called with the new text (or null). */
  onEditCell?: (row: number, col: number, value: string | null) => void;
  /** Persisted widths by column name; missing columns are measured from the data. */
  columnWidths?: Record<string, number>;
  /** Called after a drag or auto-fit with every column's width. */
  onColumnWidthsChange?: (widths: Record<string, number>) => void;
  /** The object whose rows these are; clicking its own primary key switches its view instead of opening it. */
  owner?: Address;
  /** A cell to select and scroll to, e.g. the focused value carried over from the Document view. */
  focus?: CellPos | null;
  /** The menu of a column header (right-click), opened where the pointer is. */
  columnMenu?: (col: number, at: { x: number; y: number }) => MenuEntry[] | Promise<MenuEntry[]>;
}

/** Viewport the virtualizer assumes until the first layout measurement, so the first frame already has rows. */
const INITIAL_VIEWPORT = { width: 1000, height: 800 };

const samePos = <T extends CellPos>(a: T | null, row: number, col: number): a is T => a?.row === row && a.col === col;

/** Numbers align right; everything else about a kind is drawn by `Value` (docs/ux.md, Value grammar). */
function cellClass(cell: Cell, row: number, col: number, marks: { selected: CellPos | null; editing: CellPos | null }): string {
  let name = cell.t === "number" ? "grid-cell num" : "grid-cell";
  if (samePos(marks.selected, row, col)) name += " selected";
  if (samePos(marks.editing, row, col)) name += " editing";
  return name;
}

/** The table every sourced value of a row is stored in, when they share one, for the linked table highlight. */
function rowTable(values: StoredValue[]): string | undefined {
  const tables = new Set(values.flatMap((value) => (value.source ? [value.source.table] : [])));
  return tables.size === 1 ? [...tables][0] : undefined;
}

interface KeyContext {
  at: CellPos;
  lastCol: number;
  /** Rows that fit the viewport, for PageUp / PageDown. */
  pageRows: number;
  moveTo(row: number, col: number): void;
  open(at: CellPos): void;
  copyValue(at: CellPos): void;
}

/** Grid keys by `e.key`, with `Mod+` for ⌘ chords. */
const KEY_ACTIONS: Record<string, (c: KeyContext) => void> = {
  ArrowUp: (c) => c.moveTo(c.at.row - 1, c.at.col),
  ArrowDown: (c) => c.moveTo(c.at.row + 1, c.at.col),
  ArrowLeft: (c) => c.moveTo(c.at.row, c.at.col - 1),
  ArrowRight: (c) => c.moveTo(c.at.row, c.at.col + 1),
  Home: (c) => c.moveTo(c.at.row, 0),
  End: (c) => c.moveTo(c.at.row, c.lastCol),
  PageUp: (c) => c.moveTo(c.at.row - c.pageRows, c.at.col),
  PageDown: (c) => c.moveTo(c.at.row + c.pageRows, c.at.col),
  Enter: (c) => c.open(c.at),
  "Mod+c": (c) => c.copyValue(c.at),
};

const clamp = (value: number, max: number) => Math.min(max, Math.max(0, value));

/** Scrolls `grid` sideways until column `col` shows, right of the pinned key column; its start wins when it is wider. */
function revealColumn(grid: HTMLElement | null, widths: number[], col: number, pinned: boolean) {
  if (!grid || col < 0 || col >= widths.length || (pinned && col === 0)) return;
  const start = widths.slice(0, col).reduce((sum, w) => sum + w, 0);
  const left = start - (pinned ? widths[0] : 0);
  const right = start + widths[col];
  if (left < grid.scrollLeft) grid.scrollLeft = left;
  else if (right > grid.scrollLeft + grid.clientWidth) grid.scrollLeft = Math.min(left, right - grid.clientWidth);
}

export function ResultsGrid({ columns, rows, source, valueAt, sort, onSort, rowActions, onEditCell, columnWidths, onColumnWidthsChange, owner, focus, columnMenu }: Props) {
  const selectCell = useStore((s) => s.selectCell);
  const rowHeight = useRowHeight();
  const scrollRef = useRef<HTMLDivElement>(null);
  const [selected, setSelected] = useState<CellPos | null>(null);
  const menu = useContextMenu();
  const [selectionRows, setSelectionRows] = useState(rows);
  // New rows invalidate cell positions; resetting during render keeps stale indices out of every frame.
  if (selectionRows !== rows) {
    setSelectionRows(rows);
    setSelected(null);
  }
  const { editing, startEdit, withFullText, setText, cancel, commit } = useCellEditing(rows, source, onEditCell);

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 12,
    initialRect: INITIAL_VIEWPORT,
  });
  // Row height follows the data size setting; re-measure so rows neither overlap nor gap.
  useEffect(() => virtualizer.measure(), [rowHeight, virtualizer]);

  const rowValues = useRowValues(rows, valueAt);
  const leads = rows.length > 0 ? rowValues(0).map((value) => (value && hasLeadIcon(value) ? LEAD_ICON_CH : 0)) : undefined;
  const { widths, totalWidth, colVars, startResize, autoFit } = useColumnSizing({
    columns,
    rows,
    saved: columnWidths,
    onChange: onColumnWidthsChange,
    visibleRows: () => virtualizer.getVirtualItems().map((v) => rows[v.index]),
    leads,
  });
  const names = columns.map((c) => c.name);

  // The first column stays in view while the grid scrolls sideways when it holds the rows' primary key.
  const pinned = rows.length > 0 && rowValues(0)[0]?.kind.kind === "primaryKey";

  const select = (row: number, col: number) => {
    setSelected({ row, col });
    selectCell({ row, col, column: columns[col], cell: rows[row][col], source, value: rowValues(row)[col] });
  };

  const { openValue, openRow, place, targetMenu } = useOpenValue(owner ?? null);

  const copyCell = (at: CellPos) => withFullText(at, (text) => void copyText(text, "value"));

  useEditValueCommand(source, startEdit);
  useFocusCell(focus, rows, select, (row, col) => {
    virtualizer.scrollToIndex(row);
    revealColumn(scrollRef.current, widths, col, pinned);
    scrollRef.current?.scrollIntoView?.({ block: "nearest" });
  });

  /**
   * A primary key cell opens its row as an object (double-click pinned, Enter in the preview tab). Enter on a
   * reference opens its target like a click, on serialized data or long text a value column; other cells edit.
   */
  const openOrEdit = (at: CellPos, preview: boolean) => {
    const value = rows[at.row] ? rowValues(at.row)[at.col] : null;
    const kind = value?.kind;
    if (kind?.kind === "primaryKey" && kind.address.target.kind === "row") return openRow(kind.address, preview);
    if (preview && kind?.kind === "reference" && kind.exists !== false && kind.targets.length === 1) return openTarget(kind.targets[0], place);
    if (preview && value && (kind?.kind === "serialized" || rows[at.row][at.col].t === "longText")) return withFullText(at, (text) => void showValue(value, place, text));
    startEdit(at);
  };

  const openMenu = (e: React.MouseEvent, row: number, col: number) => {
    select(row, col);
    menu.open(
      e,
      cellMenu({
        row,
        col,
        rows,
        names,
        value: rowValues(row)[col],
        copy: (text, what) => void copyText(text, what),
        copyValue: () => copyCell({ row, col }),
        edit: onEditCell && { start: () => startEdit({ row, col }), setNull: () => onEditCell(row, col, null) },
        rowActions: rowActions?.(row),
      }),
    );
  };

  const openColumnMenu = async (e: React.MouseEvent, col: number) => {
    if (!columnMenu) return;
    e.preventDefault();
    const at = { x: e.clientX, y: e.clientY };
    const items = await columnMenu(col, at);
    if (items.length > 0) menu.open(at, items);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    const action = KEY_ACTIONS[e.metaKey ? `Mod+${e.key.toLowerCase()}` : e.key];
    if (!action || !selected || rows.length === 0) return;
    e.preventDefault();
    const lastRow = rows.length - 1;
    const lastCol = columns.length - 1;
    action({
      at: { row: clamp(selected.row, lastRow), col: clamp(selected.col, lastCol) },
      lastCol,
      pageRows: Math.max(1, Math.floor((scrollRef.current?.clientHeight ?? 0) / rowHeight) - 1),
      moveTo: (row, col) => {
        const target = clamp(row, lastRow);
        const column = clamp(col, lastCol);
        select(target, column);
        virtualizer.scrollToIndex(target);
        revealColumn(scrollRef.current, widths, column, pinned);
      },
      open: (at) => openOrEdit(at, true),
      copyValue: copyCell,
    });
  };

  if (columns.length === 0) {
    return <div className="empty-state">No columns</div>;
  }

  return (
    <div
      className={pinned ? "grid pinned" : "grid"}
      data-stage="results"
      data-stage-label="Results"
      ref={scrollRef}
      tabIndex={0}
      onKeyDown={onKeyDown}
      onScroll={pinned ? markScrolled : undefined}
      role="grid"
      aria-rowcount={rows.length}
      style={colVars}
    >
      <div className="grid-head" style={{ width: totalWidth }} role="row">
        {columns.map((c, i) => {
          const sorted = sort?.column === c.name;
          return (
            <div
              key={`${c.name}-${i}`}
              role="columnheader"
              className={`grid-hcell${onSort ? " sortable" : ""}`}
              style={{ width: `var(--col-${i})` }}
              title={`${c.name} · ${c.typeName}`}
              aria-sort={sorted ? (sort?.descending ? "descending" : "ascending") : undefined}
              onClick={() => onSort?.(c.name)}
              onContextMenu={(e) => void openColumnMenu(e, i)}
            >
              <span className="name">
                {c.name}
                {sorted && (sort?.descending ? <ArrowDown size="0.769rem" /> : <ArrowUp size="0.769rem" />)}
              </span>
              <span className="type">{c.typeName.toLowerCase()}</span>
              <span
                className="col-resize"
                role="separator"
                aria-label={`Resize ${c.name}`}
                aria-orientation="vertical"
                title="Drag to resize · double-click to fit"
                onMouseDown={(e) => startResize(e, i)}
                onDoubleClick={(e) => (e.stopPropagation(), autoFit(i))}
                onClick={(e) => e.stopPropagation()}
              />
            </div>
          );
        })}
      </div>
      <div className="grid-inner" style={{ height: virtualizer.getTotalSize(), width: totalWidth }}>
        {virtualizer.getVirtualItems().map((v) => {
          const row = rows[v.index];
          const values = rowValues(v.index);
          return (
            <div key={v.key} className="grid-row" role="row" data-row-table={rowTable(values)} style={{ top: v.start, height: rowHeight, width: totalWidth }}>
              {row.map((cell, ci) => (
                <div
                  key={ci}
                  role="gridcell"
                  className={cellClass(cell, v.index, ci, { selected, editing })}
                  style={{ width: `var(--col-${ci})` }}
                  onMouseDown={() => select(v.index, ci)}
                  onDoubleClick={() => openOrEdit({ row: v.index, col: ci }, false)}
                  onContextMenu={(e) => openMenu(e, v.index, ci)}
                >
                  {samePos(editing, v.index, ci) ? (
                    <input
                      className="cell-input"
                      autoFocus
                      aria-label="Edit cell"
                      value={editing.text}
                      onChange={(e) => setText(e.target.value)}
                      onKeyDown={(e) => {
                        e.stopPropagation();
                        if (e.key === "Escape") cancel();
                        if (e.key === "Enter") commit();
                      }}
                      onBlur={cancel}
                    />
                  ) : (
                    <Value value={values[ci]} onOpen={openValue} density="grid" />
                  )}
                </div>
              ))}
            </div>
          );
        })}
      </div>
      {menu.element}
      {targetMenu}
    </div>
  );
}
