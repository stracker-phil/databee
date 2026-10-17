/** Group bodies of the Document view: key/value rows for fields and meta, a small table for row groups. */
import { memo, useMemo } from "react";
import type { Group, GroupRow, Value as StoredValue } from "../../api";
import { fieldOf, moreText, provenanceKey } from "../../lib/document";
import { cellText } from "../../lib/format";
import { Field, isNumeric } from "./fields";

/** The focused and edited values, as provenance keys. */
export interface Marks {
  focus: string | null;
  editing: string | null;
}

const keyOf = (value: StoredValue | null) => (value?.source ? provenanceKey(value.source) : null);

/** Whether `value` is the focused or the edited one. */
function marksFor(value: StoredValue | null, marks: Marks): { focused: boolean; editing: boolean } {
  const key = keyOf(value);
  return { focused: !!key && key === marks.focus, editing: !!key && key === marks.editing };
}

function KvRow({ name, value, table, marks }: { name: string; value: StoredValue | null; table: string; marks: Marks }) {
  return (
    <div className="row kv" data-row-table={table || undefined}>
      <span title={name}>{name}</span>
      <Field value={value} {...marksFor(value, marks)} />
    </div>
  );
}

/** The own row: one key/value row per column. */
export function FieldsGroup({ group, marks }: { group: Group; marks: Marks }) {
  const row = group.rows[0];
  if (!row) return null;
  return (
    <>
      {row.fields.map(([name, value]) => (
        <KvRow key={name} name={name} value={value} table={group.table} marks={marks} />
      ))}
    </>
  );
}

/** Meta rows in stored order: the key as the label, the value editable on its own meta row. */
export function MetaGroup({ group, marks }: { group: Group; marks: Marks }) {
  return (
    <>
      {group.rows.map((row, i) => {
        const keyCell = row.fields[0]?.[1];
        const value = row.fields[1]?.[1] ?? null;
        const name = keyCell ? cellText(keyCell.raw) : "";
        return <KvRow key={keyOf(value) ?? i} name={name} value={value} table={group.table} marks={marks} />;
      })}
    </>
  );
}

const MAX_CHARS = 40;

/** A column's width in characters: its name or its longest shown value, capped. */
function columnWidth(column: string, index: number, rows: GroupRow[]): string {
  let longest = column.length;
  for (const row of rows) {
    const value = fieldOf(row, column, index);
    if (value) longest = Math.max(longest, cellText(value.raw).length);
  }
  return `${Math.min(MAX_CHARS, longest) + 2}ch`;
}

/** Column widths from the header and the shown values; the last column takes the rest. */
function template(columns: string[], rows: GroupRow[]): string {
  const widths = columns.map((column, i) => columnWidth(column, i, rows));
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
    <div className="row" style={style} data-row-table={table || undefined}>
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
      {group.rows.map((row, i) => {
        const key = row.key ? row.key.map((p) => p.value).join(",") : i;
        return <TableRow key={key} row={row} columns={group.columns} table={group.table} style={style} marks={marks} />;
      })}
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
      {group.rows.map((row, i) => {
        const [name, value] = row.fields[0] ?? ["", null];
        return (
          <div key={i} className="row" style={EVENT_STYLE} data-row-table={value?.source?.table}>
            <Field value={value} focused={marksFor(value, marks).focused} />
            <span className="o2">{value?.source?.table}</span>
            <span>{name}</span>
          </div>
        );
      })}
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
