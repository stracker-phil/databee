/** One value of a document row: focusable, editable in place, drawn by `Value`. */
import { createContext, memo, useContext, useState, type KeyboardEvent } from "react";
import type { Provenance, Value as StoredValue } from "../../api";
import { cellText } from "../../lib/format";
import { Value, type OpenValue } from "../value/Value";

export interface DocumentActions {
  onOpen: OpenValue;
  focus(source: Provenance | null): void;
  /** Queues the new text (null for NULL) for the value's provenance row. */
  save(value: StoredValue, text: string | null): void;
  cancelEdit(): void;
}

const noop = () => {};

export const DocumentContext = createContext<DocumentActions>({ onOpen: noop, focus: noop, save: noop, cancelEdit: noop });

/** Numbers and money align right, as in grids. */
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

interface FieldProps {
  value: StoredValue | null;
  focused: boolean;
  editing?: boolean;
  /** Right-aligned, as numbers are in grids. */
  numeric?: boolean;
}

/** A value cell; a mousedown focuses its provenance, so value commands and a following click act on it. */
export const Field = memo(function Field({ value, focused, editing = false, numeric = false }: FieldProps) {
  const { onOpen, focus } = useContext(DocumentContext);
  if (!value) return <span />;
  if (editing) return <ValueEditor value={value} />;
  const classes = [numeric ? "r" : "", focused ? "focus" : ""].filter(Boolean).join(" ");
  return (
    <span className={classes || undefined} onMouseDown={() => focus(value.source)}>
      <Value value={value} onOpen={onOpen} />
    </span>
  );
});
