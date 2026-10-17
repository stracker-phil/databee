import { createContext, memo, useContext, useState, type KeyboardEvent, type MouseEvent } from "react";
import type { Provenance, Value as StoredValue } from "../../api";
import { FLASH_MS } from "../../lib/diff";
import { cellText } from "../../lib/format";
import { Value, type OpenValue } from "../value/Value";

export interface DocumentActions {
  onOpen: OpenValue;
  focus(source: Provenance | null): void;
  /** Queues the new text (null for NULL) for the value's provenance row. */
  save(value: StoredValue, text: string | null): void;
  cancelEdit(): void;
  /** Right-click on a sourced value, after it took the focus. */
  menu(event: MouseEvent): void;
}

const noop = () => {};

export const DocumentContext = createContext<DocumentActions>({ onOpen: noop, focus: noop, save: noop, cancelEdit: noop, menu: noop });

export const isNumeric = (value: StoredValue | null) => value?.kind.kind === "number" || value?.kind.kind === "money";

function ValueEditor({ value }: { value: StoredValue }) {
  const { save, cancelEdit } = useContext(DocumentContext);
  const [text, setText] = useState(cellText(value.raw));
  const onKeyDown = (e: KeyboardEvent) => {
    e.stopPropagation();
    if (e.key === "Escape") cancelEdit();
    if (e.key === "Enter") save(value, text);
  };
  return <input className="cell-input" autoFocus aria-label="Edit value" value={text} onChange={(e) => setText(e.target.value)} onKeyDown={onKeyDown} onBlur={cancelEdit} />;
}

/** A changed value: the old value struck through next to it (null: only the row mark), and when it changed. */
export interface FieldChange {
  old: string | null;
  /** A change seen in the last `FLASH_MS` flashes; null never flashes (compare). */
  at: number | null;
}

interface FieldProps {
  value: StoredValue | null;
  focused: boolean;
  editing?: boolean;
  numeric?: boolean;
  change?: FieldChange;
}

/** A value cell; a mousedown focuses its provenance, so value commands and a following click act on it. */
export const Field = memo(function Field({ value, focused, editing = false, numeric = false, change }: FieldProps) {
  const { onOpen, focus, menu } = useContext(DocumentContext);
  if (!value) return <span />;
  if (editing) return <ValueEditor value={value} />;
  const classes = [numeric ? "r" : "", focused ? "focus" : ""].filter(Boolean).join(" ");
  const onContextMenu = (e: MouseEvent) => {
    if (!value.source || value.raw.t === "bytes") return;
    e.preventDefault();
    focus(value.source);
    menu(e);
  };
  return (
    <span className={classes || undefined} onMouseDown={() => focus(value.source)} onContextMenu={onContextMenu}>
      {change?.at != null && Date.now() - change.at < FLASH_MS ? (
        <span key={change.at} className="flash">
          <Value value={value} onOpen={onOpen} />
        </span>
      ) : (
        <Value value={value} onOpen={onOpen} />
      )}
      {change?.old != null && <span className="old">{change.old}</span>}
    </span>
  );
});
