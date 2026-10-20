/**
 * Keys of the column stack (docs/ux.md, Keys). The bare keys act only while the keyboard focus is in the stack or
 * nowhere; the ⌘ pane keys anywhere in the object tab, those that move, close or open panes not while text is edited.
 */
import { collapseKey, documentText, focusOrder, provenanceKey } from "../lib/document";
import { maximizable, objectColumnAt } from "../lib/columns";
import { drillable, openValueAt } from "../components/value/open";
import { hasValueColumn, selectActiveTab, selectCollapsedGroups, selectDocument, selectFocusedValue, selectMaxPanes, useStore, type ObjectTab } from "../state/store";
import { isTextInput, type Command } from "./registry";

const focused = () => document.activeElement;
const typing = () => isTextInput(focused());
const inGrid = () => !!focused()?.closest('[role="grid"]');
const onDivider = () => !!focused()?.closest('[role="separator"]');
const menuOpen = () => !!document.querySelector('[role="menu"]');

/** The keyboard focus is in an object tab's stack or its trail, or on nothing in particular (the body, a hidden tab). */
function inStack(): boolean {
  const el = focused();
  return !el || el === document.body || !!el.closest(".stack, .trail, [hidden]");
}

const onControl = () => !!focused()?.closest("button, a, [role='button'], [role='link'], [role='separator']");

function activeObjectTab(): ObjectTab | null {
  const tab = selectActiveTab(useStore.getState());
  return tab?.kind === "object" ? tab : null;
}

function documentColumn() {
  const tab = activeObjectTab();
  const column = tab?.columns[tab.active];
  if (!tab || column?.kind !== "object" || column.view !== "document") return null;
  return { tab, column };
}

/** From no focus (-1) the first step lands on the first or the last value; the ends hold. */
function nextFocusIndex(current: number, delta: number, length: number): number {
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
  return !!tab && tab.columns.length > 1 && !typing() && inStack() && !(arrow && (inGrid() || onDivider()));
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

/** A column to close: one of several in an object tab, or a table or query tab's value column; or a maximized pane to restore. */
function canClose(): boolean {
  const tab = selectActiveTab(useStore.getState());
  if (!tab || typing() || menuOpen()) return false;
  if (tab.kind === "object") return tab.columns.length > 1 || !!tab.maximized;
  return hasValueColumn(tab);
}

function canMaximize(): boolean {
  const tab = activeObjectTab();
  return !!tab && !menuOpen() && maximizable(tab, selectMaxPanes(useStore.getState()));
}

function toggleMaximized() {
  const tab = activeObjectTab();
  if (tab) useStore.getState().toggleMaximized(tab.id);
}

/** Pane keys are ⌘ combinations, which fire in inputs too; they wait while text is edited or a menu is open. */
const canUsePanes = () => !typing() && !menuOpen();

const canFocusPane = () => canUsePanes() && (activeObjectTab()?.columns.length ?? 0) > 1;

/** Makes the pane left (-1) or right (+1) of the active one active; the keyboard focus moves into it from anywhere. */
function focusPane(delta: number) {
  const tab = activeObjectTab();
  if (!tab) return;
  const index = Math.max(0, Math.min(tab.columns.length - 1, tab.active + delta));
  if (index === tab.active) return;
  // The panel moves the focus to the new active column only from the stack or from nothing.
  if (!inStack()) (focused() as HTMLElement | null)?.blur();
  useStore.getState().setActiveColumn(tab.id, index);
}

/** The first column is the tab itself, so it never closes as a pane (⌘W closes the tab). */
const canCloseActive = () => canUsePanes() && (activeObjectTab()?.active ?? 0) > 0;

/** The row of the active pane (a value column's: the row it was opened from), when it is not the row the tab is rooted at. */
function activePaneRow() {
  const tab = activeObjectTab();
  const row = tab && objectColumnAt(tab.columns, tab.active);
  const root = tab && objectColumnAt(tab.columns, 0);
  if (!row || !root || documentText(row.address) === documentText(root.address)) return null;
  return row.address;
}

const canOpenInTab = () => canUsePanes() && !!activePaneRow();

function openInTab() {
  const address = activePaneRow();
  if (address) useStore.getState().openObject(address, { preview: false });
}

/** The pane commands that apply to the active pane now, for the context menu of its header. */
export function paneMenu(): { label: string; action: () => void }[] {
  const entries = [
    { label: "Open in new tab", applies: canOpenInTab(), action: openInTab },
    { label: "Close pane", applies: canCloseActive(), action: () => useStore.getState().closeActiveColumn() },
  ];
  return entries.filter((e) => e.applies).map(({ label, action }) => ({ label, action }));
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
  { id: "column.maximize", title: "Maximize or restore the active pane", scope: "object", defaultBinding: "Mod+Shift+Enter", menu: true, when: canMaximize, run: toggleMaximized },
  { id: "column.focusLeft", title: "Focus the pane on the left", scope: "object", defaultBinding: "Mod+Alt+ArrowLeft", menu: true, when: canFocusPane, run: () => focusPane(-1) },
  { id: "column.focusRight", title: "Focus the pane on the right", scope: "object", defaultBinding: "Mod+Alt+ArrowRight", menu: true, when: canFocusPane, run: () => focusPane(1) },
  { id: "column.close", title: "Close the active pane and the panes opened from it", scope: "object", defaultBinding: "Mod+Shift+W", menu: true, when: canCloseActive, run: () => useStore.getState().closeActiveColumn() },
  { id: "column.openInTab", title: "Open the active pane's row in a new tab", scope: "object", defaultBinding: "Mod+Alt+Enter", menu: true, when: canOpenInTab, run: openInTab },
  { id: "columns.close", title: "Close the rightmost column (restores a maximized pane first)", scope: "global", defaultBinding: "Escape", when: canClose, run: () => useStore.getState().closeRightmostColumn() },
];
