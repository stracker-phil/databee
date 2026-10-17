/**
 * The status bar text for a value: where it is stored, where a reference points and by which rule, and
 * what a timestamp means (docs/ux.md, Value grammar).
 */
import type { Address, Provenance, ReferenceSource, RowKey, Value } from "../api";
import { cellText } from "./format";

/** Names one row by table and key, e.g. `wp_wc_orders#id=4711`, to tell whether two places mean the same row. */
export const rowIdentity = (table: string, key: RowKey) => `${table}#${key.map((part) => `${part.column}=${part.value}`).join(",")}`;

/** `wp_wc_orders_meta.meta_value · id 9123` */
export function sourceText(source: Provenance): string {
  const key = source.row.map((part) => `${part.column} ${part.value}`).join(", ");
  return key ? `${source.table}.${source.column} · ${key}` : `${source.table}.${source.column}`;
}

function targetText(target: Address): string {
  const t = target.target;
  if (t.kind === "row") return `${t.table} #${t.key}`;
  if (t.kind !== "table") return t.kind;
  const filter = t.filter.map((f) => `${f.column} ${f.value ?? "NULL"}`).join(", ");
  return filter ? `${t.table} · ${filter}` : t.table;
}

export function bySourceText(by: ReferenceSource): string {
  switch (by.kind) {
    case "foreignKey":
      return `foreign key ${by.constraint}`;
    case "project":
      return `project ${by.id}`;
    case "preset":
      return `preset ${by.preset}`;
    case "sameName":
      return `inferred · same name${by.indexed ? " · indexed" : ""}`;
  }
}

const pad = (n: number) => String(n).padStart(2, "0");

function localTime(date: Date): string {
  const day = `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
  return `${day} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

/** `UTC 2026-10-17 10:00:00 · local 2026-10-17 12:00:00`; a zone that is not stored says so. */
function timeText(utc: number | null, zone: string | null): string | null {
  if (utc !== null) {
    const date = new Date(utc * 1000);
    return `UTC ${date.toISOString().slice(0, 19).replace("T", " ")} · local ${localTime(date)}`;
  }
  return zone === "utc" || zone === "unix" ? null : "zone not stored";
}

/** What the kind adds to the source: the reference target, the time, the currency's source. */
function kindText(value: Value): string | null {
  const kind = value.kind;
  switch (kind.kind) {
    case "reference": {
      const first = kind.targets[0]?.target;
      const missingIn = first?.kind === "row" || first?.kind === "table" ? first.table : null;
      if (kind.exists === false && missingIn) return `no row #${cellText(value.raw)} in ${missingIn}`;
      const targets = kind.targets.map(targetText).join(" | ");
      return targets ? `→ ${targets} · ${bySourceText(kind.by)}` : bySourceText(kind.by);
    }
    case "noReference":
      return `${value.raw.t === "null" ? "NULL" : cellText(value.raw)} = no reference`;
    case "timestamp":
      return timeText(kind.utc, kind.zone);
    case "money":
      return kind.currency?.source ? `currency ${sourceText(kind.currency.source)}` : null;
    default:
      return null;
  }
}

/** The provenance line of a value; null when there is nothing to say (a plain query result cell). */
export function provenanceText(value: Value): string | null {
  const parts = [value.source ? sourceText(value.source) : null, kindText(value)].filter((p): p is string => !!p);
  return parts.length > 0 ? parts.join(" · ") : null;
}
