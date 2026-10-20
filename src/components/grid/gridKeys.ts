import type { KeyboardEvent } from "react";
import type { CellPos } from "../gridHooks";

interface KeyContext {
  at: CellPos;
  lastCol: number;
  /** Rows that fit the viewport, for PageUp / PageDown. */
  pageRows: number;
  moveTo: (row: number, col: number) => void;
  open: (at: CellPos) => void;
  copyValue: (at: CellPos) => void;
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

interface GridKeyTarget {
  selected: CellPos | null;
  rowCount: number;
  colCount: number;
  viewport: HTMLElement | null;
  rowHeight: number;
  select: (row: number, col: number) => void;
  reveal: (row: number, col: number) => void;
  open: (at: CellPos) => void;
  copyValue: (at: CellPos) => void;
}

/** Runs the grid key of `e` on the selected cell, clamped to the grid. */
export function handleGridKey(e: KeyboardEvent, grid: GridKeyTarget) {
  const action = KEY_ACTIONS[e.metaKey ? `Mod+${e.key.toLowerCase()}` : e.key];
  if (!action || !grid.selected || grid.rowCount === 0) return;
  e.preventDefault();
  const lastRow = grid.rowCount - 1;
  const lastCol = grid.colCount - 1;
  action({
    at: { row: clamp(grid.selected.row, lastRow), col: clamp(grid.selected.col, lastCol) },
    lastCol,
    pageRows: Math.max(1, Math.floor((grid.viewport?.clientHeight ?? 0) / grid.rowHeight) - 1),
    moveTo: (row, col) => {
      const target = clamp(row, lastRow);
      const column = clamp(col, lastCol);
      grid.select(target, column);
      grid.reveal(target, column);
    },
    open: grid.open,
    copyValue: grid.copyValue,
  });
}
