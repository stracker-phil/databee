import { useEffect, useMemo, useRef, type UIEvent } from "react";
import type { Cell, ColumnMeta, Value as StoredValue } from "../api";
import { EDIT_VALUE_EVENT } from "../commands";
import type { CellSource } from "../lib/cells";
import { cellText } from "../lib/format";
import { cellValue } from "../lib/values";
import { useStore, type SelectedCell } from "../state/store";

export type CellPos = { row: number; col: number };

/** Width that fits the header and the given rows. */
export function fitWidth(col: ColumnMeta, index: number, rows: Cell[][], nullLabel: string, fontSize: number, cap: number, lead = 0): number {
  const charWidth = fontSize * 0.6;
  // The type sits under the name in smaller text, so the wider of the two sets the header's width.
  const header = Math.max(col.name.length, col.typeName.length * 0.85) + 2;
  const shown = (cell: Cell) => (cell.t === "null" ? nullLabel.length : cellText(cell).length);
  const longest = rows.reduce((max, r) => Math.max(max, Math.min(shown(r[index]), 60) + lead), 0);
  return Math.round(Math.min(cap, Math.max(64, Math.max(header, longest) * charWidth + 20)));
}

/** Values per row, built on first render and kept until `rows` change, so `Value` keeps its memo. */
export function useRowValues(rows: Cell[][], valueAt: ((row: number, col: number) => StoredValue) | undefined) {
  const cache = useMemo(() => ({ rows, values: new Map<number, StoredValue[]>() }), [rows]);
  return (row: number): StoredValue[] => {
    let values = cache.values.get(row);
    if (!values) {
      values = cache.rows[row].map((cell, col) => (valueAt ? valueAt(row, col) : cellValue(cell)));
      cache.values.set(row, values);
    }
    return values;
  };
}

function isOwnGrid(at: SelectedCell | null, activeTabId: string | null, source: Pick<CellSource, "sessionId" | "resultId">): at is SelectedCell {
  return !!at && at.tabId === activeTabId && at.source.sessionId === source.sessionId && at.source.resultId === source.resultId;
}

/** `value.edit` (e): the grid of the active tab that holds the selected cell starts editing it. */
export function useEditValueCommand(source: CellSource, startEdit: (at: CellPos) => void) {
  const startEditRef = useRef(startEdit);
  useEffect(() => {
    startEditRef.current = startEdit;
  });
  const { sessionId, resultId } = source;
  useEffect(() => {
    const edit = () => {
      const s = useStore.getState();
      const at = s.selectedCell;
      if (!isOwnGrid(at, s.activeTabId, { sessionId, resultId })) return;
      startEditRef.current({ row: at.row, col: at.col });
    };
    window.addEventListener(EDIT_VALUE_EVENT, edit);
    return () => window.removeEventListener(EDIT_VALUE_EVENT, edit);
  }, [sessionId, resultId]);
}

/** Selects `focus` when it changes or new rows arrive, and scrolls the grid to it. */
export function useFocusCell(focus: CellPos | null | undefined, rows: Cell[][], select: (row: number, col: number) => void, reveal: (row: number, col: number) => void) {
  const apply = useRef({ select, reveal });
  useEffect(() => {
    apply.current = { select, reveal };
  });
  const row = focus?.row ?? -1;
  const col = focus?.col ?? -1;
  useEffect(() => {
    if (row < 0 || row >= rows.length) return;
    apply.current.select(row, col);
    apply.current.reveal(row, col);
  }, [row, col, rows]);
}

/** `data-scrolled` on a box scrolled sideways, which draws the edge of its pinned key column. */
export function markScrolled(e: UIEvent<HTMLElement>) {
  e.currentTarget.toggleAttribute("data-scrolled", e.currentTarget.scrollLeft > 0);
}
