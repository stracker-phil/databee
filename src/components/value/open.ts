/**
 * What opening a value does (docs/ux.md, Value grammar): inside an object tab's column, references and other
 * rows' keys drill into a new column and serialized data opens a value column; a table tab moves to a
 * reference's target in place; elsewhere they open tabs.
 */
import { createContext } from "react";
import type { Address, Value } from "../../api";
import { fullCellText } from "../../lib/cells";
import { formatAddress } from "../../lib/address";
import { documentText } from "../../lib/document";
import { cellText } from "../../lib/format";
import { fail, selectActiveTab, useStore } from "../../state/store";
import { tabDatabase } from "../../state/tabKinds";

export interface ColumnPlace {
  tabId: string;
  index: number;
}

export const ColumnContext = createContext<ColumnPlace | null>(null);

/** Whether `address` is the row `owner` shows: its key toggles Document and Rows instead of opening anything. */
const isOwnRow = (address: Address, owner: Address | null) => !!owner && documentText(owner) === documentText(address);

/**
 * Opens the row a primary key addresses: in a column stack as the next column, else as an object tab. The key
 * of the object that `owner` shows switches that object between Document and Rows instead.
 */
export function openRowObject(address: Address, owner: Address | null, preview: boolean, place: ColumnPlace | null) {
  const s = useStore.getState();
  const tab = selectActiveTab(s);
  if (isOwnRow(address, owner)) {
    if (place) return s.toggleObjectView(place.tabId, place.index);
    if (tab?.kind === "object") return s.toggleObjectView(tab.id);
  }
  if (place) return s.drill(place.tabId, place.index, address);
  s.openObject(address, { preview });
}

/**
 * What a click that navigates does (docs/ux.md, Value grammar): `drill` opens a column (the side panel: the next
 * column of a stack, a value column), `go` moves the whole view or opens a tab. The click handlers and the
 * indicator a value shows on hover both follow it.
 */
export type ClickEffect = "drill" | "go";

/** A reference target drills into the next column of a stack when it is a row; anything else goes. */
export const targetEffect = (target: Address, place: ColumnPlace | null): ClickEffect => (place && target.target.kind === "row" ? "drill" : "go");

/**
 * The effect of a click on `value` shown in `density` inside `place`, for an object showing `owner`; `null` when
 * the click does not navigate (an enum filters, the own row's key toggles the view, a document's serialized
 * value expands in place).
 */
export function clickEffect(value: Value, density: "grid" | "full", owner: Address | null, place: ColumnPlace | null): ClickEffect | null {
  const kind = value.kind;
  if (kind.kind === "primaryKey") {
    if (kind.address.target.kind !== "row" || isOwnRow(kind.address, owner)) return null;
    return place ? "drill" : "go";
  }
  if (kind.kind === "reference") {
    if (kind.exists === false || kind.targets.length === 0) return null;
    return kind.targets.every((t) => targetEffect(t, place) === "drill") ? "drill" : "go";
  }
  if (kind.kind === "serialized") return density === "grid" ? "drill" : null;
  return null;
}

/** A reference target: the next column of a stack for a row, a table tab moved in place, else the preview tab. */
export function openTarget(target: Address, place: ColumnPlace | null) {
  const s = useStore.getState();
  if (place && targetEffect(target, place) === "drill") return s.drill(place.tabId, place.index, target);
  const tab = selectActiveTab(s);
  if (!place && tab?.kind === "table") return s.navigateTab(tab.id, target);
  void s.openAddress(formatAddress(target), { preview: true });
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
