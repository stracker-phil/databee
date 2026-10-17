import type { Cell, DocDelta, Group, Provenance, RowMark } from "../api";
import { provenanceKey } from "./document";
import { relativeTime } from "./format";

/** What the marks are against: the start of live mode, or a snapshot. */
export type MarksSince = { kind: "live" } | { kind: "snapshot"; label: string; created: number };

export interface ChangeLine {
  /** When the change was seen (ms since the epoch); changes of one poll share it. */
  at: number;
  kind: "changed" | "added" | "removed";
  table: string;
  /** The column, the meta key, or a row's key. */
  label: string;
  /** The value before (null for an added row, or a removed row that is no meta row). */
  old: Cell | null;
  /** The value after (null for a removed row, or an added row that is no meta row). */
  new: Cell | null;
  source: Provenance | null;
}

/** The marks of one object column, or of a saved query's tab. */
export interface ChangeMarks {
  since: MarksSince;
  /** Provenance key → the value before the last change and when it changed. */
  changed: Record<string, { old: Cell; at: number }>;
  /** Provenance keys of the values of rows that appeared → when. */
  added: Record<string, number>;
  /** Rows that are gone; they stay, struck through, until dismissed. */
  removed: RowMark[];
  lines: ChangeLine[];
}

export const noMarks = (since: MarksSince): ChangeMarks => ({ since, changed: {}, added: {}, removed: [], lines: [] });

const rowId = (row: RowMark) => `${row.group}|${row.key?.map((p) => `${p.column}=${p.value}`).join(",") ?? row.label}`;

const sourcesOf = (row: RowMark): Provenance[] => row.fields.flatMap(([, value]) => (value.source ? [value.source] : []));

/** The meta value of a meta row, else its first sourced value. */
function rowSource(row: RowMark): Provenance | null {
  const metaValue = row.value ? row.fields[1]?.[1].source : null;
  return metaValue ?? sourcesOf(row)[0] ?? null;
}

/** `marks` with the changes of one poll or snapshot comparison, seen at `at`. */
export function withDelta(marks: ChangeMarks, delta: DocDelta, at: number): ChangeMarks {
  const changed = { ...marks.changed };
  const added = { ...marks.added };
  let removed = marks.removed;
  const lines = [...marks.lines];
  for (const change of delta.changed) {
    changed[provenanceKey(change.source)] = { old: change.old, at };
    lines.push({ at, kind: "changed", table: change.source.table, label: change.label, old: change.old, new: change.new, source: change.source });
  }
  for (const row of delta.added) {
    for (const source of sourcesOf(row)) added[provenanceKey(source)] = at;
    removed = removed.filter((r) => rowId(r) !== rowId(row));
    lines.push({ at, kind: "added", table: row.table, label: row.label, old: null, new: row.value, source: rowSource(row) });
  }
  for (const row of delta.removed) {
    for (const source of sourcesOf(row)) {
      delete added[provenanceKey(source)];
      delete changed[provenanceKey(source)];
    }
    if (!removed.some((r) => rowId(r) === rowId(row))) removed = [...removed, row];
    lines.push({ at, kind: "removed", table: row.table, label: row.label, old: row.value, new: null, source: rowSource(row) });
  }
  return { since: marks.since, changed, added, removed, lines };
}

export const hasMarks = (marks: ChangeMarks | undefined) => !!marks && (marks.lines.length > 0 || marks.removed.length > 0);

/** What a Document view draws: changed values (with the old value, or only the mark in compare), new and removed rows. */
export interface DocumentMarks {
  /** Provenance key → the old value shown struck through (null: only marked) and when it changed (null: no flash). */
  changed: Map<string, { old: Cell | null; at: number | null }>;
  /** Provenance keys of values in new rows → when they appeared (null: no flash). */
  added: Map<string, number | null>;
  removed: Set<string>;
  /** The removed rows, drawn struck through in their groups. */
  removedRows: RowMark[];
}

export function documentMarks(marks: ChangeMarks): DocumentMarks {
  return {
    changed: new Map(Object.entries(marks.changed)),
    added: new Map(Object.entries(marks.added)),
    removed: new Set(marks.removed.flatMap((row) => sourcesOf(row).map(provenanceKey))),
    removedRows: marks.removed,
  };
}

/** The marks of one side of a compare: the values that differ and the rows only that side has. */
export function compareMarks(delta: DocDelta, side: "left" | "right"): DocumentMarks {
  const differing = delta.changed.map((c) => (side === "left" ? c.oldSource : c.source));
  const only = (side === "left" ? delta.removed : delta.added).flatMap(sourcesOf);
  const changed = new Map([...differing, ...only].map((source) => [provenanceKey(source), { old: null, at: null }] as const));
  return { changed, added: new Map(), removed: new Set(), removedRows: [] };
}

/** The groups with each removed row appended to its own group, so it shows where it was. */
export function withRemovedRows(groups: Group[], removed: RowMark[]): Group[] {
  if (removed.length === 0) return groups;
  return groups.map((group) => {
    const rows = removed.filter((row) => row.group === group.id).map((row) => ({ key: row.key, fields: row.fields }));
    return rows.length === 0 ? group : { ...group, rows: [...group.rows, ...rows] };
  });
}

/** `since live started`, `since snapshot before deploy (2 h ago)`. */
export function sinceText(since: MarksSince, now = Date.now()): string {
  if (since.kind === "live") return "since live started";
  return `since snapshot ${since.label} (${relativeTime(since.created, now)})`;
}

/** `14:02:11` in local time. */
export function clockTime(at: number): string {
  const d = new Date(at);
  return [d.getHours(), d.getMinutes(), d.getSeconds()].map((n) => String(n).padStart(2, "0")).join(":");
}

/** How long a changed value flashes, in ms (the `.flash` animation in app.css). */
export const FLASH_MS = 1600;
