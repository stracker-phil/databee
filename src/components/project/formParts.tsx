import { useEffect, type KeyboardEvent, type ReactNode } from "react";
import type { RuleTable } from "../../api";
import type { ColumnPick, FieldError, FormField } from "../../lib/ruleForms";
import { selectStructure, useStore } from "../../state/store";
import { Modal } from "../Modal";

export function useTableColumns(sessionId: string, database: string, table: string) {
  const entry = useStore((s) => (table ? selectStructure(s, sessionId, database, table) : undefined));
  const dataVersion = useStore((s) => s.dataVersion);
  useEffect(() => {
    if (table) void useStore.getState().loadStructure(sessionId, database, table);
  }, [sessionId, database, table, dataVersion]);
  const structure = entry?.structure;
  return { columns: structure?.columns.map((c) => c.name) ?? [], primaryKey: structure?.primaryKey ?? [] };
}

export function FieldMessage({ error, field }: { error: FieldError | null; field: FormField }) {
  if (error?.field !== field) return null;
  return (
    <span className="field-error" role="alert">
      {error.message}
    </span>
  );
}

interface PickerProps {
  label: string;
  anchor: string;
  sessionId: string;
  database: string;
  tables: RuleTable[];
  value: ColumnPick;
  onChange: (value: ColumnPick) => void;
  /** Selects the primary key once a newly chosen table's columns are known. */
  preferKey?: boolean;
}

/** A table select (tables rules can name) and a column select of that table. */
export function ColumnPicker({ label, anchor, sessionId, database, tables, value, onChange, preferKey = false }: PickerProps) {
  const { columns, primaryKey } = useTableColumns(sessionId, database, value.table);
  useEffect(() => {
    if (value.column || columns.length === 0) return;
    const key = preferKey && primaryKey.length === 1 ? primaryKey[0] : columns[0];
    onChange({ ...value, column: key });
  }, [value, columns, primaryKey, preferKey, onChange]);
  const known = tables.some((t) => t.table === value.table);
  return (
    <span className="picker">
      <select className="input mono" aria-label={`${label} table`} data-stage={`${anchor}-table`} value={value.table} onChange={(e) => onChange({ table: e.target.value, column: "" })}>
        <option value="">table…</option>
        {!known && value.table && <option value={value.table}>{value.table}</option>}
        {tables.map((t) => (
          <option key={t.table} value={t.table}>
            {t.table}
          </option>
        ))}
      </select>
      <select className="input mono" aria-label={`${label} column`} data-stage={`${anchor}-column`} value={value.column} onChange={(e) => onChange({ ...value, column: e.target.value })}>
        <option value="">column…</option>
        {value.column && !columns.includes(value.column) && <option value={value.column}>{value.column}</option>}
        {columns.map((c) => (
          <option key={c} value={c}>
            {c}
            {primaryKey.length === 1 && primaryKey[0] === c ? " (primary key)" : ""}
          </option>
        ))}
      </select>
    </span>
  );
}

export function Labeled({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="form-field">
      <span className="o3">{label}</span>
      {children}
    </div>
  );
}

interface FrameProps {
  title: string;
  opener: string;
  anchor: string;
  children: ReactNode;
  footer: ReactNode;
  saving: boolean;
  error: FieldError | null;
  onCancel: () => void;
  onSave: () => void;
}

/** A modal sheet styled like the palette: ⎋ closes it, ⌘↵ saves from any field. */
export function FormFrame({ title, opener, anchor, children, footer, saving, error, onCancel, onSave }: FrameProps) {
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key !== "Enter" || !e.metaKey) return;
    e.preventDefault();
    if (!saving) onSave();
  };
  return (
    <Modal onClose={onCancel} variant="panel" className="palette rule-sheet" label={title} stageWindow="rule-form">
      <section className="rule-form" aria-label={title} data-stage={anchor} data-stage-label={title} onKeyDown={onKeyDown}>
        <div className="sheet-head">
          <span className="sheet-title">{title}</span>
          <span className="o3">from {opener}</span>
        </div>
        <div className="form-grid">{children}</div>
        <div className="form-foot">
          {footer}
          <FieldMessage error={error} field="form" />
          <span className="spacer" />
          <button className="act o2" data-stage={`${anchor}:cancel`} onClick={onCancel}>
            Cancel <span className="kbd" aria-hidden="true">⎋</span>
          </button>
          <button className="act" data-stage={`${anchor}:save`} disabled={saving} onClick={onSave}>
            Save to project <span className="kbd" aria-hidden="true">⌘↵</span>
          </button>
        </div>
      </section>
    </Modal>
  );
}
