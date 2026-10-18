/**
 * What opening a value does (docs/ux.md, Value grammar): inside an object tab's column, references and other
 * rows' keys drill into a new column and serialized data opens a value column; a table tab moves to a
 * reference's target in place; elsewhere they open tabs.
 */
import { createContext, useContext } from "react";
import type { Address, Value } from "../../api";
import { fullCellText } from "../../lib/cells";
import { formatAddress } from "../../lib/address";
import { documentText } from "../../lib/document";
import { cellText } from "../../lib/format";
import { fail, selectActiveTab, useStore, type TableTab } from "../../state/store";
import { tabDatabase } from "../../state/tabKinds";

export interface ColumnPlace {
  tabId: string;
  index: number;
}

export const ColumnContext = createContext<ColumnPlace | null>(null);

/**
 * Opens the row a primary key addresses: in a column stack as the next column, else as an object tab. The key
 * of the object that `owner` shows switches that object between Document and Rows instead.
 */
export function openRowObject(address: Address, owner: Address | null, preview: boolean, place: ColumnPlace | null) {
  const s = useStore.getState();
  const tab = selectActiveTab(s);
  if (owner && documentText(owner) === documentText(address)) {
    if (place) return s.toggleObjectView(place.tabId, place.index);
    if (tab?.kind === "object") return s.toggleObjectView(tab.id);
  }
  if (place) return s.drill(place.tabId, place.index, address);
  s.openObject(address, { preview });
}

/**
 * How a reference opens its target: `drill` as the next column (the side panel), `go` by moving the whole
 * table or object tab to it. A click drills in a column stack and goes elsewhere.
 */
export type OpenMode = "drill" | "go";

export const clickMode = (place: ColumnPlace | null): OpenMode => (place ? "drill" : "go");

/** The row `value` is stored in, when a single-column key names it, as an address in table tab `tab`. */
function sourceRow(value: Value | null | undefined, tab: TableTab): Address | null {
  const row = value?.source?.row;
  if (!value?.source || row?.length !== 1) return null;
  const connection = useStore.getState().sessions.find((x) => x.id === tab.sessionId)?.slug;
  if (!connection) return null;
  return { connection, database: tab.database, target: { kind: "row", table: value.source.table, key: row[0].value }, view: null, compare: null };
}

/**
 * A reference target. In a column stack `drill` opens it as the next column and `go` moves the stack's tab to
 * it; in a table tab `go` moves the tab to it and `drill` moves the tab to `from`'s row with the target as the
 * next column. Without a mode, or with nowhere to move, it opens a column of the stack, else the preview tab.
 */
export function openTarget(target: Address, place: ColumnPlace | null, mode?: OpenMode, from?: Value | null) {
  const s = useStore.getState();
  const row = target.target.kind === "row";
  if (place) {
    if (mode === "go") return s.navigateTab(place.tabId, target);
    if (row) return s.drill(place.tabId, place.index, target);
  }
  const tab = selectActiveTab(s);
  if (!place && tab?.kind === "table") {
    const source = mode === "drill" && row ? sourceRow(from, tab) : null;
    if (source) return s.navigateTab(tab.id, source, target);
    if (mode === "go") return s.navigateTab(tab.id, target);
  }
  void s.openAddress(formatAddress(target), { preview: true });
}

/** The modes a reference's icons offer here: both in a column stack and for a table tab's addressable rows, else `go`. */
export function useOpenModes(value: Value): OpenMode[] {
  const place = useContext(ColumnContext);
  const inTable = useStore((s) => selectActiveTab(s)?.kind === "table");
  if (place || (inTable && value.source?.row.length === 1)) return ["go", "drill"];
  return ["go"];
}

/** The full text of `value`: a selected grid cell's `longText` is fetched in full. */
async function fullText(value: Value): Promise<string> {
  const selected = useStore.getState().selectedCell;
  if (selected?.value === value) return fullCellText(selected.cell, selected.source, selected.row, selected.col);
  return cellText(value.raw);
}

/** Opens `value` as a value column: after the column it is in, or to the right of a table or query tab. */
export async function showValue(value: Value, place: ColumnPlace | null, text?: string) {
  const tab = selectActiveTab(useStore.getState());
  if (!tab) return;
  try {
    const full = text ?? (await fullText(value));
    useStore.getState().openValueColumn(place?.tabId ?? tab.id, place?.index ?? null, { kind: "value", value, text: full, database: tabDatabase(useStore.getState(), tab, place?.index) });
  } catch (e) {
    fail(e, "Could not load the value");
  }
}

/** Whether Enter on `value` drills or opens something: one reference target, a row's key, serialized data. */
export function drillable(value: Value): boolean {
  const kind = value.kind;
  if (kind.kind === "reference") return kind.exists !== false && kind.targets.length === 1;
  if (kind.kind === "primaryKey") return kind.address.target.kind === "row";
  return kind.kind === "serialized";
}

export function openValueAt(value: Value, owner: Address, place: ColumnPlace) {
  const kind = value.kind;
  if (kind.kind === "reference" && kind.targets.length === 1) return openTarget(kind.targets[0], place);
  if (kind.kind === "primaryKey") return openRowObject(kind.address, owner, true, place);
  if (kind.kind === "serialized") return void showValue(value, place);
}
