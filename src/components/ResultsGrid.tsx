import { useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp } from "lucide-react";
import type { Cell, ColumnMeta } from "../api";
import { cellDisplay, cellText, rowsToJson, rowsToTsv } from "../lib/format";
import { rowHeightFor } from "../lib/theme";
import { useStore } from "../state/store";
import { ContextMenu, type MenuEntry } from "./ContextMenu";

interface Props {
  columns: ColumnMeta[];
  rows: Cell[][];
  /** Row number of the first row (for paged table views). */
  rowOffset?: number;
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
}

const ROW_NUMBER_WIDTH = 52;
const MIN_WIDTH_REM = 3;
const MAX_FIT_REM = 40;

const remPx = () => parseFloat(getComputedStyle(document.documentElement).fontSize) || 13;

/** Width that fits the header and the given rows; one character is ~0.6 of the data font size. */
function fitWidth(col: ColumnMeta, index: number, rows: Cell[][], nullLabel: string, fontSize: number, cap: number): number {
  const charWidth = fontSize * 0.6;
  const header = col.name.length + col.typeName.length * 0.8 + 2;
  const longest = rows.reduce((max, r) => Math.max(max, Math.min(cellDisplay(r[index], nullLabel, 60).length, 60)), 0);
  return Math.round(Math.min(cap, Math.max(64, Math.max(header, longest) * charWidth + 20)));
}

export function ResultsGrid({ columns, rows, rowOffset = 0, sort, onSort, rowActions, onEditCell, columnWidths, onColumnWidthsChange }: Props) {
  const settings = useStore((s) => s.settings);
  const selectCell = useStore((s) => s.selectCell);
  const toast = useStore((s) => s.toast);
  const nullLabel = settings?.nullLabel ?? "NULL";
  const maxChars = settings?.maxCellChars ?? 500;
  const dataFontSize = settings?.dataFontSize ?? 12;
  const rowHeight = rowHeightFor({ dataFontSize, density: settings?.density ?? "comfortable" });
  const scrollRef = useRef<HTMLDivElement>(null);
  const [selected, setSelected] = useState<{ row: number; col: number } | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuEntry[] } | null>(null);
  const [editing, setEditing] = useState<{ row: number; col: number; text: string } | null>(null);
  /** Widths changed in this grid (drag / auto-fit); win over saved and measured widths. */
  const [manual, setManual] = useState<Record<string, number>>({});

  const measured = useMemo(() => columns.map((c, i) => fitWidth(c, i, rows.slice(0, 50), nullLabel, dataFontSize, 380)), [columns, rows, nullLabel, dataFontSize]);
  const widths = columns.map((c, i) => manual[c.name] ?? columnWidths?.[c.name] ?? measured[i]);
  const totalWidth = ROW_NUMBER_WIDTH + widths.reduce((a, b) => a + b, 0);
  const names = columns.map((c) => c.name);
  // Per-column CSS variables on the grid root; cells only reference them.
  const colVars = Object.fromEntries(widths.map((w, i) => [`--col-${i}`, `${w}px`])) as React.CSSProperties;

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 12,
    initialRect: { width: 1000, height: 800 },
  });
  // Row height follows the data size setting; re-measure so rows neither overlap nor gap.
  useEffect(() => virtualizer.measure(), [rowHeight, virtualizer]);

  const commitWidths = (next: Record<string, number>) => {
    setManual(next);
    onColumnWidthsChange?.(Object.fromEntries(columns.map((c, i) => [c.name, next[c.name] ?? widths[i]])));
  };

  const startResize = (e: React.MouseEvent, index: number) => {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const startWidth = widths[index];
    const min = MIN_WIDTH_REM * remPx();
    const name = columns[index].name;
    let latest = startWidth;
    const move = (ev: MouseEvent) => {
      latest = Math.max(min, Math.round(startWidth + ev.clientX - startX));
      setManual((m) => ({ ...m, [name]: latest }));
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      commitWidths({ ...manual, [name]: latest });
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  const autoFit = (index: number) => {
    const visible = virtualizer.getVirtualItems().map((v) => rows[v.index]);
    const sample = visible.length ? visible : rows.slice(0, 50);
    const width = fitWidth(columns[index], index, sample, nullLabel, dataFontSize, MAX_FIT_REM * remPx());
    commitWidths({ ...manual, [columns[index].name]: Math.max(MIN_WIDTH_REM * remPx(), width) });
  };

  const copy = (text: string, what: string) => {
    void navigator.clipboard.writeText(text).then(
      () => toast("success", `Copied ${what}`),
      () => toast("error", "Clipboard is not available"),
    );
  };

  const select = (row: number, col: number) => {
    setSelected({ row, col });
    selectCell({ column: columns[col], cell: rows[row][col] });
  };

  const openMenu = (e: React.MouseEvent, row: number, col: number) => {
    e.preventDefault();
    select(row, col);
    setMenu({
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: "Copy value", action: () => copy(cellText(rows[row][col]), "value") },
        ...(onEditCell
          ? ([
              { label: "Edit value", action: () => setEditing({ row, col, text: cellText(rows[row][col]) }) },
              { label: "Set to NULL", action: () => onEditCell(row, col, null) },
            ] as MenuEntry[])
          : []),
        { label: "Copy row as TSV", action: () => copy(rowsToTsv(names, [rows[row]]), "row") },
        { label: "Copy row as JSON", action: () => copy(rowsToJson(names, [rows[row]]), "row as JSON") },
        "separator",
        { label: `Copy all ${rows.length} rows as TSV`, action: () => copy(rowsToTsv(names, rows), `${rows.length} rows`) },
        { label: `Copy all ${rows.length} rows as JSON`, action: () => copy(rowsToJson(names, rows), `${rows.length} rows as JSON`) },
        ...(rowActions ? (["separator", ...rowActions(row)] as MenuEntry[]) : []),
      ],
    });
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (!selected) return;
    if (e.metaKey && e.key === "c") {
      e.preventDefault();
      copy(cellText(rows[selected.row][selected.col]), "value");
      return;
    }
    const moves: Record<string, [number, number]> = {
      ArrowUp: [-1, 0],
      ArrowDown: [1, 0],
      ArrowLeft: [0, -1],
      ArrowRight: [0, 1],
    };
    const move = moves[e.key];
    if (!move) return;
    e.preventDefault();
    const row = Math.min(rows.length - 1, Math.max(0, selected.row + move[0]));
    const col = Math.min(columns.length - 1, Math.max(0, selected.col + move[1]));
    select(row, col);
    virtualizer.scrollToIndex(row);
  };

  if (columns.length === 0) {
    return <div className="empty-state">No columns</div>;
  }

  return (
    <div className="grid" ref={scrollRef} tabIndex={0} onKeyDown={onKeyDown} role="grid" aria-rowcount={rows.length} style={colVars}>
      <div className="grid-head" style={{ width: totalWidth }} role="row">
        <div className="grid-hcell grid-rownum" style={{ width: ROW_NUMBER_WIDTH }} />
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
            >
              <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{c.name}</span>
              <span className="type">{c.typeName.toLowerCase()}</span>
              {sorted && (sort?.descending ? <ArrowDown size="0.846rem" /> : <ArrowUp size="0.846rem" />)}
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
          return (
            <div key={v.key} className="grid-row" role="row" style={{ top: v.start, height: rowHeight, width: totalWidth }}>
              <div className="grid-cell grid-rownum" style={{ width: ROW_NUMBER_WIDTH }}>
                {rowOffset + v.index + 1}
              </div>
              {row.map((cell, ci) => (
                <div
                  key={ci}
                  role="gridcell"
                  className={`grid-cell t-${cell.t}${selected?.row === v.index && selected.col === ci ? " selected" : ""}${editing?.row === v.index && editing.col === ci ? " editing" : ""}`}
                  style={{ width: `var(--col-${ci})` }}
                  onMouseDown={() => select(v.index, ci)}
                  onDoubleClick={() => onEditCell && setEditing({ row: v.index, col: ci, text: cellText(cell) })}
                  onContextMenu={(e) => openMenu(e, v.index, ci)}
                >
                  {editing?.row === v.index && editing.col === ci ? (
                    <input
                      className="cell-input"
                      autoFocus
                      aria-label="Edit cell"
                      value={editing.text}
                      onChange={(e) => setEditing({ ...editing, text: e.target.value })}
                      onKeyDown={(e) => {
                        e.stopPropagation();
                        if (e.key === "Escape") setEditing(null);
                        if (e.key === "Enter") {
                          onEditCell?.(editing.row, editing.col, editing.text);
                          setEditing(null);
                        }
                      }}
                      onBlur={() => setEditing(null)}
                    />
                  ) : (
                    cellDisplay(cell, nullLabel, maxChars)
                  )}
                </div>
              ))}
            </div>
          );
        })}
      </div>
      {menu && <ContextMenu {...menu} onClose={() => setMenu(null)} />}
    </div>
  );
}
