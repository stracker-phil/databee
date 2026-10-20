import type { MouseEvent } from "react";
import type { Cell, Value as StoredValue } from "../../api";
import { copyText } from "../../lib/clipboard";
import { useContextMenu, type MenuEntry } from "../ContextMenu";
import type { CellPos } from "../gridHooks";
import { cellMenu } from "./cellMenu";

interface GridMenuOptions {
  rows: Cell[][];
  names: string[];
  rowValues: (row: number) => StoredValue[];
  select: (row: number, col: number) => void;
  copyCell: (at: CellPos) => void;
  startEdit: (at: CellPos) => void;
  onEditCell?: (row: number, col: number, value: string | null) => void;
  rowActions?: (row: number) => MenuEntry[];
  columnMenu?: (col: number, at: { x: number; y: number }) => MenuEntry[] | Promise<MenuEntry[]>;
}

/** The context menus of a grid's cells and column headers. */
export function useGridMenus({ rows, names, rowValues, select, copyCell, startEdit, onEditCell, rowActions, columnMenu }: GridMenuOptions) {
  const menu = useContextMenu();

  const openCellMenu = (e: MouseEvent, row: number, col: number) => {
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

  const openColumnMenu = async (e: MouseEvent, col: number) => {
    if (!columnMenu) return;
    e.preventDefault();
    const at = { x: e.clientX, y: e.clientY };
    const items = await columnMenu(col, at);
    if (items.length > 0) menu.open(at, items);
  };

  return { openCellMenu, openColumnMenu, menuElement: menu.element };
}
