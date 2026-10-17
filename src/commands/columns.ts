/** Keys of the column stack (docs/ux.md, Keys); they act only while the keyboard focus is in the stack or nowhere. */
import { collapseKey, documentText, focusOrder, provenanceKey } from "../lib/document";
import { objectColumnAt } from "../lib/columns";
import { drillable, openValueAt } from "../components/value/open";
import { isWpView, selectActiveTab, selectCollapsedGroups, selectDocument, selectFocusedValue, useStore, type ObjectTab } from "../state/store";
import { isTextInput, type Command } from "./registry";

const focused = () => document.activeElement;
const typing = () => isTextInput(focused());
const inGrid = () => !!focused()?.closest('[role="grid"]');
const menuOpen = () => !!document.querySelector('[role="menu"]');

/** The keyboard focus is in an object tab's stack, or on nothing in particular (the body, a hidden tab). */
function inStack(): boolean {
  const el = focused();
  return !el || el === document.body || !!el.closest(".stack, [hidden]");
}

const onControl = () => !!focused()?.closest("button, a, [role='button'], [role='link']");

function activeObjectTab(): ObjectTab | null {
  const tab = selectActiveTab(useStore.getState());
  return tab?.kind === "object" ? tab : null;
}

/** The active column, when it shows a document. */
function documentColumn() {
  const tab = activeObjectTab();
  const column = tab?.columns[tab.active];
  if (!tab || column?.kind !== "object" || column.view !== "document") return null;
  return { tab, column };
}

/** From no focus (-1) the first step lands on the first or the last value; the ends hold. */
export function nextFocusIndex(current: number, delta: number, length: number): number {
  if (current < 0) return delta > 0 ? 0 : length - 1;
  return Math.max(0, Math.min(length - 1, current + delta));
}

function moveFocus(delta: number) {
  const found = documentColumn();
  if (!found) return;
  const { tab, column } = found;
  const s = useStore.getState();
  const loaded = selectDocument(s, tab.sessionId, documentText(column.address))?.document;
  if (!loaded) return;
  const collapsed = selectCollapsedGroups(s, column.address);
  const order = focusOrder(loaded, (groupId) => collapsed.includes(collapseKey(column.address, groupId)));
  if (order.length === 0) return;
  const focusKey = column.focus ? provenanceKey(column.focus) : null;
  const current = order.findIndex((p) => provenanceKey(p) === focusKey);
  s.focusObject(tab.id, order[nextFocusIndex(current, delta, order.length)], tab.active);
}

const canMoveFocus = () => !typing() && inStack() && !inGrid() && !!documentColumn();

function canMoveColumn(arrow: boolean) {
  const tab = activeObjectTab();
  return !!tab && tab.columns.length > 1 && !typing() && inStack() && !(arrow && inGrid());
}

function drill() {
  const tab = activeObjectTab();
  const value = selectFocusedValue(useStore.getState());
  const owner = tab && objectColumnAt(tab.columns, tab.active);
  if (tab && value && owner) openValueAt(value, owner.address, { tabId: tab.id, index: tab.active });
}

function canDrill(): boolean {
  if (typing() || !inStack() || inGrid() || onControl() || !documentColumn()) return false;
  const value = selectFocusedValue(useStore.getState());
  return !!value && drillable(value);
}

/** A column to close: one of several in an object tab, or a table or query tab's value column. */
function canClose(): boolean {
  const s = useStore.getState();
  const tab = selectActiveTab(s);
  if (!tab || isWpView(s) || typing() || menuOpen()) return false;
  if (tab.kind === "object") return tab.columns.length > 1;
  return (tab.kind === "table" || tab.kind === "query") && !!tab.valueColumn;
}

const vertical = (id: string, title: string, key: string, delta: number): Command => ({ id, title, scope: "object", defaultBinding: key, when: canMoveFocus, run: () => moveFocus(delta) });

const sideways = (id: string, title: string, key: string, delta: number): Command => ({
  id,
  title,
  scope: "object",
  defaultBinding: key,
  when: () => canMoveColumn(key.startsWith("Arrow")),
  run: () => useStore.getState().moveActiveColumn(delta),
});

export const COLUMN_COMMANDS: Command[] = [
  vertical("object.down", "Focus the next value", "J", 1),
  vertical("object.downArrow", "Focus the next value (arrow)", "ArrowDown", 1),
  vertical("object.up", "Focus the previous value", "K", -1),
  vertical("object.upArrow", "Focus the previous value (arrow)", "ArrowUp", -1),
  sideways("object.left", "Move to the column on the left", "H", -1),
  sideways("object.leftArrow", "Move to the column on the left (arrow)", "ArrowLeft", -1),
  sideways("object.right", "Move to the column on the right", "L", 1),
  sideways("object.rightArrow", "Move to the column on the right (arrow)", "ArrowRight", 1),
  { id: "object.drill", title: "Open the focused reference in a new column", scope: "object", defaultBinding: "Enter", when: canDrill, run: drill },
  { id: "columns.close", title: "Close the rightmost column", scope: "global", defaultBinding: "Escape", when: canClose, run: () => useStore.getState().closeRightmostColumn() },
];
