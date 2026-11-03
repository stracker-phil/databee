import type { MouseEvent } from "react";
import type { Cell, Value as StoredValue } from "../../api";
import { FLASH_MS } from "../../lib/diff";
import type { RowMark } from "../../lib/pendingMarks";
import type { RowChange } from "../../lib/tableCheck";
import type { CellPos } from "../gridHooks";
import { Value, type OpenValue } from "../value/Value";

/** The changed cell last hovered; its overlay shows while ⌥ peeks it. */
export type Peek = CellPos & { cell: Element };

/** The stored value of a cell the database changed, once the check read it. */
export function storedCell(change: RowChange | undefined, col: number): Cell | undefined {
  return change?.kind === "changed" && change.columns.includes(col) ? change.cells?.[col] : undefined;
}

const rowChangeClass = (change: RowChange | undefined) => (change ? (change.kind === "removed" ? " db-removed" : " db-changed") : "");

const pendingClass = (mark: RowMark | undefined) => (mark?.deleted ? " pending-delete" : "") + (mark?.duplicated ? " pending-duplicate" : "");

/** The table every sourced value of a row is stored in, when they share one, for the linked table highlight. */
function rowTable(values: StoredValue[]): string | undefined {
  const tables = new Set(values.flatMap((value) => (value.source ? [value.source.table] : [])));
  return tables.size === 1 ? [...tables][0] : undefined;
}

/** Numbers align right; everything else about a kind is drawn by `Value` (docs/ux.md, Value grammar). */
interface CellState {
  selected: boolean;
  editing: boolean;
  /** When the database changed the value (live check). */
  changedAt: number | undefined;
  pendingEdit: boolean;
}

function cellClass(cell: Cell, { selected, editing, changedAt, pendingEdit }: CellState): string {
  let name = cell.t === "number" ? "grid-cell num" : "grid-cell";
  if (selected) name += " selected";
  if (editing) name += " editing";
  if (pendingEdit) name += " pending-edit";
  if (changedAt !== undefined) name += Date.now() - changedAt < FLASH_MS ? " db-changed db-flash" : " db-changed";
  return name;
}

interface CellEditor {
  text: string;
  setText: (text: string) => void;
  cancel: () => void;
  commit: () => void;
}

export interface CellHandlers {
  select: (row: number, col: number) => void;
  openOrEdit: (at: CellPos, preview: boolean) => void;
  openMenu: (e: MouseEvent, row: number, col: number) => void;
  setPeek: (peek: Peek | null) => void;
}

interface RowProps {
  index: number;
  cells: Cell[];
  values: StoredValue[];
  change: RowChange | undefined;
  pending: RowMark | undefined;
  top: number;
  height: number;
  width: number;
  /** The selected column when the selection is in this row. */
  selectedCol: number | null;
  /** The inline editor when it is open in this row. */
  editor: (CellEditor & { col: number }) | null;
  handlers: CellHandlers;
  openValue: OpenValue;
}

export function GridRow({ index, cells, values, change, pending, top, height, width, selectedCol, editor, handlers, openValue }: RowProps) {
  return (
    <div
      className={`grid-row${rowChangeClass(change)}${pendingClass(pending)}`}
      role="row"
      title={pending?.title}
      data-row-table={rowTable(values)}
      style={{ top, height, width }}
    >
      {cells.map((cell, ci) => (
        <GridCell
          key={ci}
          row={index}
          col={ci}
          cell={cell}
          value={values[ci]}
          change={change}
          pendingEdit={!!pending?.columns.has(ci)}
          selected={selectedCol === ci}
          editor={editor?.col === ci ? editor : null}
          handlers={handlers}
          openValue={openValue}
        />
      ))}
    </div>
  );
}

interface CellProps {
  row: number;
  col: number;
  cell: Cell;
  value: StoredValue;
  change: RowChange | undefined;
  pendingEdit: boolean;
  selected: boolean;
  editor: CellEditor | null;
  handlers: CellHandlers;
  openValue: OpenValue;
}

function GridCell({ row, col, cell, value, change, pendingEdit, selected, editor, handlers, openValue }: CellProps) {
  const stored = storedCell(change, col);
  return (
    <div
      role="gridcell"
      className={cellClass(cell, { selected, editing: editor !== null, changedAt: stored ? change?.at : undefined, pendingEdit })}
      style={{ width: `var(--col-${col})` }}
      data-db-change={stored ? "" : undefined}
      onMouseEnter={stored ? (e) => handlers.setPeek({ row, col, cell: e.currentTarget }) : undefined}
      onMouseDown={() => handlers.select(row, col)}
      onDoubleClick={() => handlers.openOrEdit({ row, col }, false)}
      onContextMenu={(e) => handlers.openMenu(e, row, col)}
    >
      {editor ? (
        <input
          className="cell-input"
          autoFocus
          aria-label="Edit cell"
          value={editor.text}
          onChange={(e) => editor.setText(e.target.value)}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Escape") editor.cancel();
            if (e.key === "Enter") editor.commit();
          }}
          onBlur={editor.cancel}
        />
      ) : (
        <Value value={value} onOpen={openValue} density="grid" />
      )}
    </div>
  );
}
