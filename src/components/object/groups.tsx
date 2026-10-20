import { memo, useMemo } from "react";
import type { Group, GroupRow, Value as StoredValue } from "../../api";
import type { DocumentMarks } from "../../lib/diff";
import { fieldOf, moreText, provenanceKey } from "../../lib/document";
import { cellText, charWidth } from "../../lib/format";
import { LongList } from "../LongList";
import { hasLeadIcon, LEAD_ICON_CH } from "../value/Value";
import { Field, isNumeric, type FieldChange } from "./fields";
import { WideTable } from "./WideTable";

/** The focused and edited values, as provenance keys, and the change marks of live mode, a snapshot or compare. */
export interface Marks {
  focus: string | null;
  editing: string | null;
  diff?: DocumentMarks | null;
}

const keyOf = (value: StoredValue | null) => (value?.source ? provenanceKey(value.source) : null);

export const oldText = (cell: StoredValue["raw"]) => (cell.t === "null" ? "NULL" : cellText(cell));

function changeOf(key: string | null, diff: DocumentMarks | null | undefined): FieldChange | undefined {
  if (!key || !diff) return undefined;
  const changed = diff.changed.get(key);
  if (changed) return { old: changed.old && oldText(changed.old), at: changed.at };
  if (diff.added.has(key)) return { old: "no row", at: diff.added.get(key) ?? null };
  return undefined;
}

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

const leads = (rows: GroupRow[], column: string, index: number) =>
  rows.some((row) => {
    const value = fieldOf(row, column, index);
    return !!value && hasLeadIcon(value);
  });

/** Column widths from the header and the shown values, with room for a leading table icon. */
const columnWidths = (columns: string[], rows: GroupRow[]) =>
  columns.map((column, i) => charWidth(column, columnTexts(rows, column, i), 40, leads(rows, column, i) ? LEAD_ICON_CH : 0));

/** The rows' first column is their primary key, which stays in view while the group scrolls sideways. */
const keyFirst = (group: Group) => {
  const first = group.rows[0];
  return !!first && fieldOf(first, group.columns[0], 0)?.kind.kind === "primaryKey";
};

interface TableRowProps {
  row: GroupRow;
  columns: string[];
  table: string;
  marks: Marks;
}

const TableRow = memo(function TableRow({ row, columns, table, marks }: TableRowProps) {
  return (
    <div className={`row${rowClass(row.fields.map(([, value]) => value), marks.diff)}`} data-row-table={table || undefined}>
      {columns.map((column, i) => {
        const value = fieldOf(row, column, i);
        return <Field key={column} value={value} {...marksFor(value, marks)} numeric={isNumeric(value)} />;
      })}
    </div>
  );
});

export function RowsGroup({ group, marks }: { group: Group; marks: Marks }) {
  const widths = useMemo(() => columnWidths(group.columns, group.rows), [group]);
  const first = group.rows[0];
  return (
    <WideTable widths={widths} pinned={keyFirst(group)}>
      <div className="row hdr">
        {group.columns.map((column, i) => (
          <span key={column} className={first && isNumeric(fieldOf(first, column, i)) ? "r" : undefined}>
            {column}
          </span>
        ))}
      </div>
      <LongList items={group.rows} revealWhere={(row) => holdsFocus(row, marks)}>
        {(row, i) => {
          const key = row.key ? row.key.map((p) => p.value).join(",") : i;
          return <TableRow key={key} row={row} columns={group.columns} table={group.table} marks={marks} />;
        }}
      </LongList>
    </WideTable>
  );
}

const EVENT_WIDTHS = ["22ch", "32ch", "20ch"];

export function EventsGroup({ group, marks }: { group: Group; marks: Marks }) {
  return (
    <WideTable widths={EVENT_WIDTHS}>
      <div className="row hdr">
        <span>time</span>
        <span>table</span>
        <span>column or key</span>
      </div>
      <LongList items={group.rows} revealWhere={(row) => holdsFocus(row, marks)}>
        {(row, i) => {
          const [name, value] = row.fields[0] ?? ["", null];
          return (
            <div key={i} className="row" data-row-table={value?.source?.table}>
              <Field value={value} focused={marksFor(value, marks).focused} />
              <span className="o2">{value?.source?.table}</span>
              <span>{name}</span>
            </div>
          );
        }}
      </LongList>
    </WideTable>
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
