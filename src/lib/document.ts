import type { Address, Document, Group, GroupRow, Provenance, Value } from "../api";
import { formatAddress } from "./address";
import { formatCount, plural } from "./format";
import { bySourceText } from "./provenance";

export type ObjectView = "document" | "rows";

/** A row address showing `view`; the document is the default and has no parameter. */
export function objectAddress(address: Address, view: ObjectView): Address {
  return { ...address, view: view === "rows" ? "rows" : null, compare: null };
}

/** What identifies a document, whatever view shows it. */
export const documentText = (address: Address) => formatAddress(objectAddress(address, "document"));

/** `wp_wc_orders #4711`, or `order #4711` once the entity is known. */
export function objectTitle(address: Address, entity?: string | null): string {
  const target = address.target;
  if (target.kind !== "row") return formatAddress(address);
  return `${entity ?? target.table} #${target.key}`;
}

/** `table.column:key` of a provenance, for comparing and as a React key. */
export function provenanceKey(source: Provenance): string {
  return `${source.table}.${source.column}:${source.row.map((p) => `${p.column}=${p.value}`).join(",")}`;
}

function search(document: Document, key: string): Value | null {
  for (const group of document.groups) {
    for (const row of group.rows) {
      for (const [, value] of row.fields) {
        if (value.source && provenanceKey(value.source) === key) return value;
      }
    }
  }
  return null;
}

let lastDocument: Document | null = null;
let lastKey = "";
let lastValue: Value | null = null;

/** The value stored at `source` anywhere in the document; the last lookup is remembered (the status bar asks often). */
export function findValue(document: Document, source: Provenance): Value | null {
  const key = provenanceKey(source);
  if (document === lastDocument && key === lastKey) return lastValue;
  lastDocument = document;
  lastKey = key;
  lastValue = search(document, key);
  return lastValue;
}

/** The value of `column` in a group row; fields follow the group's columns, so the index usually matches. */
export function fieldOf(row: GroupRow, column: string, index: number): Value | null {
  const at = row.fields[index];
  if (at && at[0] === column) return at[1];
  return row.fields.find(([name]) => name === column)?.[1] ?? null;
}

/** The relation in a section band, mirroring the group's link (docs/ux.md, Section header band). */
export function relationText(group: Group): string {
  const via = group.via;
  switch (via.kind) {
    case "own":
      return "this row";
    case "meta":
      return `meta · ${via.fk}`;
    case "incoming": {
      const link = `${via.reference.from.columns.join(", ")} → this row`;
      const inferred = via.reference.by.kind === "sameName" ? " · inferred" : "";
      return `${via.label ? `${via.label} · ` : ""}${link}${inferred}`;
    }
    case "pattern":
      return `pattern · ${via.sql}`;
    case "events":
      return "every stored timestamp, in order";
  }
}

/** Where the group's rule comes from, for the status bar while the band is hovered. */
export function groupOriginText(group: Group): string | null {
  const via = group.via;
  if (via.kind === "incoming") {
    const { from, to, by } = via.reference;
    return `${from.table}.${from.columns.join(", ")} → ${to.table}.${to.columns.join(", ")} · ${bySourceText(by)}`;
  }
  if (via.kind === "meta") return `${group.table}.${via.key} → ${group.table}.${via.value} · ${via.fk} → this row`;
  if (via.kind === "pattern") return `pattern ${via.rule} · ${via.sql}`;
  return null;
}


/** `13 columns`, `9 keys · stored order`, `2 rows`. */
export function countText(group: Group): string {
  if (group.shape === "fields") return plural(group.columns.length, "column", "columns");
  if (group.shape === "meta") return `${plural(group.total, "key", "keys")} · stored order`;
  if (group.via.kind === "events") return formatCount(group.total);
  return plural(group.total, "row", "rows");
}

/** `+3 keys` / `+12 rows` for a capped group; null when nothing is cut. */
export function moreText(group: Group): string | null {
  const more = group.total - group.rows.length;
  if (!group.capped || more <= 0) return null;
  return `+${group.shape === "meta" ? plural(more, "key", "keys") : plural(more, "row", "rows")}`;
}

/** A group with no rows, which collapses into the "Also linked" line; a failed group keeps its band. */
export const isEmpty = (group: Group) => group.total === 0 && group.rows.length === 0 && !group.error && group.via.kind !== "own";

/** The `UiState.collapsedGroups` key of a group: per table of the document, so it holds for every row of it. */
export function collapseKey(owner: Address, groupId: string): string {
  const table = "table" in owner.target ? owner.target.table : "";
  return `${table}::${groupId}`;
}

/** The values of one group in the order the Document view shows them; `events` repeats values shown elsewhere. */
function groupValues(group: Group): (Value | null)[] {
  if (group.via.kind === "events") return [];
  if (group.shape === "fields") return group.rows[0]?.fields.map(([, value]) => value) ?? [];
  if (group.shape === "meta") return group.rows.map((row) => row.fields[1]?.[1] ?? null);
  return group.rows.flatMap((row) => group.columns.map((column, i) => fieldOf(row, column, i)));
}

/** The provenance of every value `j`/`k` move through, top to bottom, skipping collapsed groups and repeats. */
export function focusOrder(document: Document, collapsed: (groupId: string) => boolean): Provenance[] {
  const seen = new Set<string>();
  const order: Provenance[] = [];
  for (const group of document.groups) {
    if (isEmpty(group) || group.error || collapsed(group.id)) continue;
    for (const value of groupValues(group)) {
      const source = value?.source;
      if (!source || seen.has(provenanceKey(source))) continue;
      seen.add(provenanceKey(source));
      order.push(source);
    }
  }
  return order;
}

/** Stage anchors per group: `group-<table>`, with the group id added for a second group of the same table. */
export function groupAnchors(groups: Group[]): Map<string, string> {
  const seen = new Set<string>();
  const anchors = new Map<string, string>();
  for (const group of groups) {
    const name = group.table || group.id;
    anchors.set(group.id, seen.has(name) ? `group-${name}:${group.id}` : `group-${name}`);
    seen.add(name);
  }
  return anchors;
}
