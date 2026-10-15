import { useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp } from "lucide-react";
import type { Cell, ColumnMeta } from "../api";
import { cellDisplay, cellText, rowsToJson, rowsToTsv } from "../lib/format";
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
}

const ROW_NUMBER_WIDTH = 52;
const CHAR_WIDTH = 7.2;

function columnWidths(columns: ColumnMeta[], rows: Cell[][], nullLabel: string): number[] {
  const sample = rows.slice(0, 50);
  return columns.map((col, i) => {
    const header = col.name.length + col.typeName.length * 0.8 + 2;
    const longest = sample.reduce((max, r) => Math.max(max, Math.min(cellDisplay(r[i], nullLabel, 60).length, 60)), 0);
    return Math.round(Math.min(380, Math.max(64, Math.max(header, longest) * CHAR_WIDTH + 20)));
  });
}

export function ResultsGrid({ columns, rows, rowOffset = 0, sort, onSort, rowActions, onEditCell }: Props) {
  const settings = useStore((s) => s.settings);
  const selectCell = useStore((s) => s.selectCell);
  const toast = useStore((s) => s.toast);
  const nullLabel = settings?.nullLabel ?? "NULL";
  const maxChars = settings?.maxCellChars ?? 500;
  const rowHeight = settings?.density === "compact" ? 21 : 26;
  const scrollRef = useRef<HTMLDivElement>(null);
  const [selected, setSelected] = useState<{ row: number; col: number } | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuEntry[] } | null>(null);
  const [editing, setEditing] = useState<{ row: number; col: number; text: string } | null>(null);

  const widths = useMemo(() => columnWidths(columns, rows, nullLabel), [columns, rows, nullLabel]);
  const totalWidth = ROW_NUMBER_WIDTH + widths.reduce((a, b) => a + b, 0);
  const names = columns.map((c) => c.name);

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 12,
    initialRect: { width: 1000, height: 800 },
  });

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
    <div className="grid" ref={scrollRef} tabIndex={0} onKeyDown={onKeyDown} role="grid" aria-rowcount={rows.length}>
      <div className="grid-head" style={{ width: totalWidth }} role="row">
        <div className="grid-hcell grid-rownum" style={{ width: ROW_NUMBER_WIDTH }} />
        {columns.map((c, i) => {
          const sorted = sort?.column === c.name;
          return (
            <div
              key={`${c.name}-${i}`}
              role="columnheader"
              className={`grid-hcell${onSort ? " sortable" : ""}`}
              style={{ width: widths[i] }}
              title={`${c.name} · ${c.typeName}`}
              aria-sort={sorted ? (sort?.descending ? "descending" : "ascending") : undefined}
              onClick={() => onSort?.(c.name)}
            >
              <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{c.name}</span>
              <span className="type">{c.typeName.toLowerCase()}</span>
              {sorted && (sort?.descending ? <ArrowDown size={11} /> : <ArrowUp size={11} />)}
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
                  className={`grid-cell t-${cell.t}${selected?.row === v.index && selected.col === ci ? " selected" : ""}`}
                  style={{ width: widths[ci] }}
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
