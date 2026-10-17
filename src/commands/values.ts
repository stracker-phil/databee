/** Value commands (`y`, `Y`, `e`, `f`) on the focused value: the selected grid cell (docs/ux.md, Keys). */
import { api, type Provenance, type Value } from "../api";
import { fullCellText } from "../lib/cells";
import { copyText } from "../lib/clipboard";
import { fail, useStore, type SelectedCell, type TableTab } from "../state/store";
import type { Command } from "./registry";

/** Window event: the grid holding the selected cell opens its inline editor there. */
export const EDIT_VALUE_EVENT = "databee:edit-value";

type Focused = SelectedCell & { value: Value };

const hasValue = (selected: SelectedCell | null): selected is Focused => !!selected?.value;

function focused(): Focused | null {
  const selected = useStore.getState().selectedCell;
  return hasValue(selected) ? selected : null;
}

/** The focused value with its table tab; values of query results have none. */
function inTable(): { selected: Focused; tab: TableTab } | null {
  const s = useStore.getState();
  const selected = focused();
  const tab = s.tabs.find((t) => t.id === selected?.tabId);
  return selected && tab?.kind === "table" ? { selected, tab } : null;
}

/** A sourced value in a table tab; a binary cell holds only a preview, which must not become a condition. */
function sourced(): { selected: Focused; tab: TableTab; source: Provenance } | null {
  const found = inTable();
  const source = found?.selected.value.source;
  if (!found || !source || found.selected.cell.t === "bytes") return null;
  return { ...found, source };
}

/** The full stored text, fetched for a `longText` cell; null for NULL. */
async function rawText(selected: Focused): Promise<string | null> {
  if (selected.cell.t === "null") return null;
  return fullCellText(selected.cell, selected.source, selected.row, selected.col);
}

async function attempt(context: string, action: () => Promise<void>) {
  try {
    await action();
  } catch (e) {
    fail(e, context);
  }
}

async function copyRaw() {
  const selected = focused();
  if (!selected) return;
  await attempt("Could not load the value", async () => copyText((await rawText(selected)) ?? "", "value"));
}

async function copyCondition() {
  const found = sourced();
  if (!found) return;
  const { selected, tab, source } = found;
  await attempt("Could not build the condition", async () => {
    const sql = await api.valueCondition(tab.sessionId, tab.database, source, await rawText(selected));
    await copyText(sql, "condition");
  });
}

/** Adds `column = value` of the focused value to its table tab's structured filter. */
async function filterByValue() {
  const found = sourced();
  if (!found) return;
  const { selected, tab, source } = found;
  await attempt("Could not load the value", async () => {
    useStore.getState().filterRows(tab.id, { column: source.column, value: await rawText(selected) });
  });
}

function editable(): boolean {
  const found = inTable();
  return !!found && found.selected.cell.t !== "bytes";
}

export const VALUE_COMMANDS: Command[] = [
  { id: "value.copyRaw", title: "Copy value", scope: "table", defaultBinding: "Y", when: () => !!focused(), run: copyRaw },
  { id: "value.copyCondition", title: "Copy value as SQL condition", scope: "table", defaultBinding: "Shift+Y", when: () => !!sourced(), run: copyCondition },
  { id: "value.edit", title: "Edit value", scope: "table", defaultBinding: "E", when: editable, run: () => void window.dispatchEvent(new Event(EDIT_VALUE_EVENT)) },
  { id: "value.filter", title: "Filter rows by value", scope: "table", defaultBinding: "F", when: () => !!sourced(), run: filterByValue },
];
