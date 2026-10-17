/**
 * What opening a value does (docs/ux.md, Value grammar): inside an object tab's column, references and other
 * rows' keys drill into a new column and serialized data opens a value column; elsewhere they open tabs.
 */
import { createContext } from "react";
import type { Address, Value } from "../../api";
import { fullCellText } from "../../lib/cells";
import { objectColumnAt } from "../../lib/columns";
import { formatAddress } from "../../lib/address";
import { documentText } from "../../lib/document";
import { cellText } from "../../lib/format";
import { fail, selectActiveSession, selectActiveTab, useStore, type Tab } from "../../state/store";

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

/** A reference target: the next column of a stack, else the preview tab. */
export function openTarget(target: Address, place: ColumnPlace | null) {
  if (place && target.target.kind === "row") return useStore.getState().drill(place.tabId, place.index, target);
  void useStore.getState().openAddress(formatAddress(target), { preview: true });
}

function databaseOf(tab: Tab, place: ColumnPlace | null): string | null {
  if (tab.kind === "object") return objectColumnAt(tab.columns, place?.index ?? tab.active)?.address.database ?? null;
  if (tab.kind === "table") return tab.database;
  return selectActiveSession(useStore.getState())?.database ?? null;
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
    useStore.getState().openValueColumn(place?.tabId ?? tab.id, place?.index ?? null, { kind: "value", value, text: full, database: databaseOf(tab, place) });
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
