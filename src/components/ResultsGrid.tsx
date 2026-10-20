import { useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp } from "lucide-react";
import type { Address, Cell, ColumnMeta, Value as StoredValue } from "../api";
import type { CellSource } from "../lib/cells";
import { copyText } from "../lib/clipboard";
import type { RowChange } from "../lib/tableCheck";
import { useRowHeight } from "../lib/theme";
import { useStore } from "../state/store";
import type { MenuEntry } from "./ContextMenu";
import { ChangePeek } from "./grid/ChangePeek";
import { handleGridKey } from "./grid/gridKeys";
import { GridRow, storedCell, type CellHandlers, type Peek } from "./grid/GridRow";
import { useCellEditing } from "./grid/useCellEditing";
import { useColumnSizing } from "./grid/useColumnSizing";
import { useGridMenus } from "./grid/useGridMenus";
import { markScrolled, useEditValueCommand, useFocusCell, useRowValues, type CellPos } from "./gridHooks";
import { openTarget, showValue } from "./value/open";
import { useOpenValue } from "./value/useOpenValue";
import { hasLeadIcon, LEAD_ICON_CH } from "./value/Value";

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
  /** Rows the database changed or removed since they were read, by row index (a table tab's live check). */
  changes?: Readonly<Record<number, RowChange>>;
  /** Called when the inline editor opens (true) or closes (false). */
  onEditingChange?: (editing: boolean) => void;
}

/** Viewport the virtualizer assumes until the first layout measurement, so the first frame already has rows. */
const INITIAL_VIEWPORT = { width: 1000, height: 800 };

function useEditingChange(editing: boolean, onEditingChange: ((editing: boolean) => void) | undefined) {
  const notify = useRef(onEditingChange);
  useEffect(() => {
    notify.current = onEditingChange;
  });
  useEffect(() => notify.current?.(editing), [editing]);
}

/** Scrolls `grid` until row `row` shows between the sticky header `head` and the bottom edge; rows are `rowHeight` tall. */
function revealRow(grid: HTMLElement | null, head: HTMLElement | null, row: number, rowHeight: number) {
  if (!grid) return;
  const top = row * rowHeight;
  const bottom = (head?.offsetHeight ?? 0) + top + rowHeight;
  if (top < grid.scrollTop) grid.scrollTop = top;
  else if (bottom > grid.scrollTop + grid.clientHeight) grid.scrollTop = Math.min(bottom, grid.scrollHeight) - grid.clientHeight;
}

/** Scrolls `grid` sideways until column `col` shows, right of the pinned key column; its start wins when it is wider. */
function revealColumn(grid: HTMLElement | null, widths: number[], col: number, pinned: boolean) {
  if (!grid || col < 0 || col >= widths.length || (pinned && col === 0)) return;
  const start = widths.slice(0, col).reduce((sum, w) => sum + w, 0);
  const left = start - (pinned ? widths[0] : 0);
  const right = start + widths[col];
  if (left < grid.scrollLeft) grid.scrollLeft = left;
  else if (right > grid.scrollLeft + grid.clientWidth) grid.scrollLeft = Math.min(left, right - grid.clientWidth);
}

export function ResultsGrid({ columns, rows, source, valueAt, sort, onSort, rowActions, onEditCell, columnWidths, onColumnWidthsChange, owner, focus, columnMenu, changes, onEditingChange }: Props) {
  const selectCell = useStore((s) => s.selectCell);
  const rowHeight = useRowHeight();
  const scrollRef = useRef<HTMLDivElement>(null);
  const headRef = useRef<HTMLDivElement>(null);
  const [selected, setSelected] = useState<CellPos | null>(null);
  const [selectionRows, setSelectionRows] = useState(rows);
  // New rows invalidate cell positions; resetting during render keeps stale indices out of every frame.
  if (selectionRows !== rows) {
    setSelectionRows(rows);
    setSelected(null);
  }
  const { editing, startEdit, withFullText, setText, cancel, commit } = useCellEditing(rows, source, onEditCell);
  useEditingChange(editing !== null, onEditingChange);
  const [peek, setPeek] = useState<Peek | null>(null);
  const peekStored = peek && rows[peek.row] ? storedCell(changes?.[peek.row], peek.col) : undefined;

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

  const reveal = (row: number, col: number) => {
    revealRow(scrollRef.current, headRef.current, row, rowHeight);
    revealColumn(scrollRef.current, widths, col, pinned);
  };

  useEditValueCommand(source, startEdit);
  useFocusCell(focus, rows, select, (row, col) => {
    reveal(row, col);
    scrollRef.current?.scrollIntoView?.({ block: "nearest" });
  });

  /** Opens a key, reference, serialized or long text cell (`preview`: Enter) or edits it. */
  const openOrEdit = (at: CellPos, preview: boolean) => {
    const value = rows[at.row] ? rowValues(at.row)[at.col] : null;
    const kind = value?.kind;
    if (kind?.kind === "primaryKey" && kind.address.target.kind === "row") return openRow(kind.address, preview);
    if (preview && kind?.kind === "reference" && kind.exists !== false && kind.targets.length === 1) return openTarget(kind.targets[0], place);
    if (preview && value && (kind?.kind === "serialized" || rows[at.row][at.col].t === "longText")) return withFullText(at, (text) => void showValue(value, place, text));
    startEdit(at);
  };

  const { openCellMenu, openColumnMenu, menuElement } = useGridMenus({ rows, names, rowValues, select, copyCell, startEdit, onEditCell, rowActions, columnMenu });

  const onKeyDown = (e: React.KeyboardEvent) =>
    handleGridKey(e, {
      selected,
      rowCount: rows.length,
      colCount: columns.length,
      viewport: scrollRef.current,
      rowHeight,
      select,
      reveal,
      open: (at) => openOrEdit(at, true),
      copyValue: copyCell,
    });

  const handlers: CellHandlers = { select, openOrEdit, openMenu: openCellMenu, setPeek };
  const editor = editing && { ...editing, setText, cancel, commit };

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
      onScroll={(e) => {
        if (pinned) markScrolled(e);
        if (peek) setPeek(null);
      }}
      role="grid"
      aria-rowcount={rows.length}
      style={colVars}
    >
      <div className="grid-head" ref={headRef} style={{ width: totalWidth }} role="row">
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
        {virtualizer.getVirtualItems().map((v) => (
          <GridRow
            key={v.key}
            index={v.index}
            cells={rows[v.index]}
            values={rowValues(v.index)}
            change={changes?.[v.index]}
            top={v.start}
            height={rowHeight}
            width={totalWidth}
            selectedCol={selected?.row === v.index ? selected.col : null}
            editor={editor?.row === v.index ? editor : null}
            handlers={handlers}
            openValue={openValue}
          />
        ))}
      </div>
      {menuElement}
      {targetMenu}
      {peek && peekStored && <ChangePeek column={columns[peek.col].name} shown={rows[peek.row][peek.col]} stored={peekStored} cell={peek.cell} />}
    </div>
  );
}
