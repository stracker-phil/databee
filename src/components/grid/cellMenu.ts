import type { Cell, Value as StoredValue } from "../../api";
import { RULE_MENU } from "../../commands";
import { rowsToJson, rowsToTsv } from "../../lib/format";
import type { MenuEntry } from "../ContextMenu";

export interface CellMenuContext {
  row: number;
  col: number;
  rows: Cell[][];
  names: string[];
  value: StoredValue | undefined;
  copy: (text: string, what: string) => void;
  copyValue: () => void;
  /** Present when the grid edits cells. */
  edit?: { start: () => void; setNull: () => void };
  rowActions?: MenuEntry[];
}

/** The context menu of a grid cell. Bytes cells hold only a preview, so they can't be edited or ruled. */
export function cellMenu({ row, col, rows, names, value, copy, copyValue, edit, rowActions }: CellMenuContext): MenuEntry[] {
  const bytes = rows[row][col].t === "bytes";
  return [
    { label: "Copy value", action: copyValue },
    ...(edit
      ? ([
          { label: "Edit value", disabled: bytes, action: edit.start },
          { label: "Set to NULL", action: edit.setNull },
        ] as MenuEntry[])
      : []),
    ...(value?.source && !bytes ? (["separator", ...RULE_MENU, "separator"] as MenuEntry[]) : []),
    { label: "Copy row as TSV", action: () => copy(rowsToTsv(names, [rows[row]]), "row") },
    { label: "Copy row as JSON", action: () => copy(rowsToJson(names, [rows[row]]), "row as JSON") },
    "separator",
    { label: `Copy all ${rows.length} rows as TSV`, action: () => copy(rowsToTsv(names, rows), `${rows.length} rows`) },
    { label: `Copy all ${rows.length} rows as JSON`, action: () => copy(rowsToJson(names, rows), `${rows.length} rows as JSON`) },
    ...(rowActions ? (["separator", ...rowActions] as MenuEntry[]) : []),
  ];
}
