/** Group bodies of the Document view: key/value rows for fields and meta, a small table for row groups. */
import { memo, useMemo } from "react";
import type { Group, GroupRow, Value as StoredValue } from "../../api";
import type { DocumentMarks } from "../../lib/diff";
import { fieldOf, moreText, provenanceKey } from "../../lib/document";
import { cellText, charWidth } from "../../lib/format";
import { LongList } from "../LongList";
import { Field, isNumeric, type FieldChange } from "./fields";

/** The focused and edited values, as provenance keys, and the change marks of live mode, a snapshot or compare. */
export interface Marks {
  focus: string | null;
  editing: string | null;
  diff?: DocumentMarks | null;
}

const keyOf = (value: StoredValue | null) => (value?.source ? provenanceKey(value.source) : null);

/** The old value as the change list writes it. */
export const oldText = (cell: StoredValue["raw"]) => (cell.t === "null" ? "NULL" : cellText(cell));

function changeOf(key: string | null, diff: DocumentMarks | null | undefined): FieldChange | undefined {
  if (!key || !diff) return undefined;
  const changed = diff.changed.get(key);
  if (changed) return { old: changed.old && oldText(changed.old), at: changed.at };
  if (diff.added.has(key)) return { old: "no row", at: diff.added.get(key) ?? null };
  return undefined;
}

/** Whether `value` is the focused or the edited one, and how it changed. */
function marksFor(value: StoredValue | null, marks: Marks): { focused: boolean; editing: boolean; change?: FieldChange } {
  const key = keyOf(value);
  return { focused: !!key && key === marks.focus, editing: !!key && key === marks.editing, change: changeOf(key, marks.diff) };
}

/** `chg` for a row with a changed or new value (a 3px accent mark), `removed` for a row that is gone. */
function rowClass(values: (StoredValue | null)[], diff: DocumentMarks | null | undefined): string {
  if (!diff) return "";
  const keys = values.map(keyOf).filter((key): key is string => !!key);
  if (keys.some((key) => diff.removed.has(key))) return " removed";
  if (keys.some((key) => diff.changed.has(key) || diff.added.has(key))) return " chg";
  return "";
}

function KvRow({ name, value, table, marks }: { name: string; value: StoredValue | null; table: string; marks: Marks }) {
  return (
    <div className={`row kv${rowClass([value], marks.diff)}`} data-row-table={table || undefined}>
      <span title={name}>{name}</span>
      <Field value={value} {...marksFor(value, marks)} />
    </div>
  );
}

const isFocused = (value: StoredValue | null, marks: Marks) => !!marks.focus && keyOf(value) === marks.focus;
const holdsFocus = (row: GroupRow, marks: Marks) => row.fields.some(([, value]) => isFocused(value, marks));

/** The own row: one key/value row per column. */
export function FieldsGroup({ group, marks }: { group: Group; marks: Marks }) {
  const row = group.rows[0];
  if (!row) return null;
  return (
    <LongList items={row.fields} revealWhere={([, value]) => isFocused(value, marks)}>
      {([name, value]) => <KvRow key={name} name={name} value={value} table={group.table} marks={marks} />}
    </LongList>
  );
}

/** Meta rows in stored order: the key as the label, the value editable on its own meta row. */
export function MetaGroup({ group, marks }: { group: Group; marks: Marks }) {
  return (
    <LongList items={group.rows} revealWhere={(row) => holdsFocus(row, marks)}>
      {(row, i) => {
        const keyCell = row.fields[0]?.[1];
        const value = row.fields[1]?.[1] ?? null;
        const name = keyCell ? cellText(keyCell.raw) : "";
        return <KvRow key={keyOf(value) ?? i} name={name} value={value} table={group.table} marks={marks} />;
      }}
    </LongList>
  );
}

function* columnTexts(rows: GroupRow[], column: string, index: number) {
  for (const row of rows) {
    const value = fieldOf(row, column, index);
    if (value) yield cellText(value.raw);
  }
}

/** Column widths from the header and the shown values; the last column takes the rest. */
function template(columns: string[], rows: GroupRow[]): string {
  const widths = columns.map((column, i) => charWidth(column, columnTexts(rows, column, i)));
  if (widths.length > 0) widths[widths.length - 1] = "minmax(0, 1fr)";
  return widths.join(" ");
}

interface TableRowProps {
  row: GroupRow;
  columns: string[];
  table: string;
  style: React.CSSProperties;
  marks: Marks;
}

const TableRow = memo(function TableRow({ row, columns, table, style, marks }: TableRowProps) {
  return (
    <div className={`row${rowClass(row.fields.map(([, value]) => value), marks.diff)}`} style={style} data-row-table={table || undefined}>
      {columns.map((column, i) => {
        const value = fieldOf(row, column, i);
        return <Field key={column} value={value} {...marksFor(value, marks)} numeric={isNumeric(value)} />;
      })}
    </div>
  );
});

/** Rows of an incoming reference or a pattern: a header row of column names, then the rows. */
export function RowsGroup({ group, marks }: { group: Group; marks: Marks }) {
  const style = useMemo(() => ({ gridTemplateColumns: template(group.columns, group.rows) }), [group]);
  const first = group.rows[0];
  return (
    <>
      <div className="row hdr" style={style}>
        {group.columns.map((column, i) => (
          <span key={column} className={first && isNumeric(fieldOf(first, column, i)) ? "r" : undefined}>
            {column}
          </span>
        ))}
      </div>
      <LongList items={group.rows} revealWhere={(row) => holdsFocus(row, marks)}>
        {(row, i) => {
          const key = row.key ? row.key.map((p) => p.value).join(",") : i;
          return <TableRow key={key} row={row} columns={group.columns} table={group.table} style={style} marks={marks} />;
        }}
      </LongList>
    </>
  );
}

const EVENT_STYLE = { gridTemplateColumns: "22ch 32ch minmax(0, 1fr)" };

/** Every timestamp of the document in order: the time, the table it is stored in, the column or meta key. */
export function EventsGroup({ group, marks }: { group: Group; marks: Marks }) {
  return (
    <>
      <div className="row hdr" style={EVENT_STYLE}>
        <span>time</span>
        <span>table</span>
        <span>column or key</span>
      </div>
      <LongList items={group.rows} revealWhere={(row) => holdsFocus(row, marks)}>
        {(row, i) => {
          const [name, value] = row.fields[0] ?? ["", null];
          return (
            <div key={i} className="row" style={EVENT_STYLE} data-row-table={value?.source?.table}>
              <Field value={value} focused={marksFor(value, marks).focused} />
              <span className="o2">{value?.source?.table}</span>
              <span>{name}</span>
            </div>
          );
        }}
      </LongList>
    </>
  );
}

/** `+n keys` / `+n rows` under a capped group; opens the Rows view at this group. */
export function MoreRows({ group, onMore }: { group: Group; onMore: (groupId: string) => void }) {
  const text = moreText(group);
  if (!text) return null;
  return (
    <button className="more" data-stage={`more:${group.id}`} data-stage-label={text} onClick={() => onMore(group.id)}>
      {text}
    </button>
  );
}
