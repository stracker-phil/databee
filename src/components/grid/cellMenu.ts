import type { Cell, Value as StoredValue } from "../../api";
import { commandItem } from "../../commands";
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
  /** Whose value commands the items run and show the keys of: a grid's (`value.*`) or the Document view's (`object.*`). */
  commands?: "value" | "object";
}

/**
 * The context menu of a value in a grid or the Document view. Bytes cells hold only a preview, so they can't be
 * edited or ruled; a sourced one can still be watched.
 */
export function cellMenu({ row, col, rows, names, value, copy, copyValue, edit, rowActions, commands = "value" }: CellMenuContext): MenuEntry[] {
  const bytes = rows[row][col].t === "bytes";
  const id = (name: string) => `${commands}.${name}`;
  const rules: MenuEntry[] = bytes ? [] : [commandItem(id("addReference"), "Add reference…"), commandItem(id("setKind"), "Kind…"), commandItem(id("setStyle"), "Style…")];
  return [
    { label: "Copy value", action: copyValue, command: id("copyRaw") },
    ...(edit
      ? ([
          { label: "Edit value", disabled: bytes, action: edit.start, command: id("edit") },
          { label: "Set to NULL", action: edit.setNull, command: commands === "value" ? "value.setNull" : undefined },
        ] as MenuEntry[])
      : []),
    ...(value?.source ? (["separator", commandItem(id("watch"), "Watch on readout"), ...rules, "separator"] as MenuEntry[]) : []),
    { label: "Copy row as TSV", action: () => copy(rowsToTsv(names, [rows[row]]), "row") },
    { label: "Copy row as JSON", action: () => copy(rowsToJson(names, [rows[row]]), "row as JSON") },
    "separator",
    { label: `Copy all ${rows.length} rows as TSV`, action: () => copy(rowsToTsv(names, rows), `${rows.length} rows`) },
    { label: `Copy all ${rows.length} rows as JSON`, action: () => copy(rowsToJson(names, rows), `${rows.length} rows as JSON`) },
    ...(rowActions ? (["separator", ...rowActions] as MenuEntry[]) : []),
  ];
}
