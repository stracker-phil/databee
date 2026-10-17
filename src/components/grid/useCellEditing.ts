import { useEffect, useRef, useState } from "react";
import { toAppError, type Cell } from "../../api";
import { fullCellText, type CellSource } from "../../lib/cells";
import { cellText } from "../../lib/format";
import { useStore } from "../../state/store";
import type { CellPos } from "../gridHooks";

/** The inline editor of a grid; closed whenever `rows` changes identity. */
export function useCellEditing(rows: Cell[][], source: CellSource, onEditCell: ((row: number, col: number, value: string | null) => void) | undefined) {
  const toast = useStore((s) => s.toast);
  const [editing, setEditing] = useState<(CellPos & { text: string }) | null>(null);
  const [editRows, setEditRows] = useState(rows);
  // An edit pre-filled from `fetchCell` must not open on rows that replaced the ones it was for.
  const currentRows = useRef(rows);
  useEffect(() => {
    currentRows.current = rows;
  }, [rows]);
  if (editRows !== rows) {
    setEditRows(rows);
    setEditing(null);
  }

  // Bytes cells carry a preview, not their value; editing it would write the preview back. Long text is fetched first.
  const canEdit = (cell: Cell) => !!onEditCell && cell.t !== "bytes";

  /** Calls `apply` with the cell's full text: right away, or after fetching a `longText` cell. */
  const withFullText = ({ row, col }: CellPos, apply: (text: string) => void) => {
    const cell = rows[row][col];
    if (cell.t !== "longText") return apply(cellText(cell));
    fullCellText(cell, source, row, col).then(apply, (e) => toast("error", `Could not load the value: ${toAppError(e).message}`));
  };

  const startEdit = ({ row, col }: CellPos) => {
    const cell = rows[row]?.[col];
    if (!cell || !canEdit(cell)) return;
    const shown = rows;
    withFullText({ row, col }, (text) => currentRows.current === shown && setEditing({ row, col, text }));
  };

  const setText = (text: string) => setEditing((e) => e && { ...e, text });
  const cancel = () => setEditing(null);
  const commit = () => {
    if (editing) onEditCell?.(editing.row, editing.col, editing.text);
    setEditing(null);
  };

  return { editing, startEdit, withFullText, setText, cancel, commit };
}
