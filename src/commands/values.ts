/**
 * Value commands (`y`, `Y`, `e`, `f`, `r`, "Kind…", "Style…") on the focused value: the selected grid cell of the active tab, or the
 * focused value of an object's document (docs/ux.md, Keys). The table and object scopes each have a set.
 */
import { api, type Provenance, type Value } from "../api";
import { fullCellText } from "../lib/cells";
import { copyText } from "../lib/clipboard";
import { cellText } from "../lib/format";
import { kindFormFor, referenceFormFor, styleFormFor } from "../lib/ruleForms";
import { fail, selectActiveSession, selectActiveTab, selectFocusedValue, useStore, type Store, type Tab } from "../state/store";
import type { Command, Scope } from "./registry";

/** Window event: the view holding the focused value (a grid, a document) opens its inline editor there. */
export const EDIT_VALUE_EVENT = "databee:edit-value";

interface Focused {
  value: Value;
  tab: Tab;
  database: string | null;
  /** The full stored text (a grid's `longText` cell fetched first); null for NULL. */
  raw(): Promise<string | null>;
}

function databaseOf(s: Store, tab: Tab): string | null {
  if (tab.kind === "table") return tab.database;
  if (tab.kind === "object") return tab.address.database;
  return selectActiveSession(s)?.database ?? null;
}

function focused(): Focused | null {
  const s = useStore.getState();
  const tab = selectActiveTab(s);
  const value = selectFocusedValue(s);
  if (!tab || !value) return null;
  const selected = s.selectedCell?.tabId === tab.id ? s.selectedCell : null;
  const raw = async () => {
    if (value.raw.t === "null") return null;
    return selected ? fullCellText(selected.cell, selected.source, selected.row, selected.col) : cellText(value.raw);
  };
  return { value, tab, database: databaseOf(s, tab), raw };
}

/** A sourced value; a binary cell holds only a preview, which must not become a condition or an edit. */
function sourced(): (Focused & { source: Provenance; database: string }) | null {
  const found = focused();
  const source = found?.value.source;
  if (!found || !source || !found.database || found.value.raw.t === "bytes") return null;
  return { ...found, source, database: found.database };
}

async function attempt(context: string, action: () => Promise<void>) {
  try {
    await action();
  } catch (e) {
    fail(e, context);
  }
}

async function copyRaw() {
  const found = focused();
  if (!found) return;
  await attempt("Could not load the value", async () => copyText((await found.raw()) ?? "", "value"));
}

async function copyCondition() {
  const found = sourced();
  if (!found) return;
  await attempt("Could not build the condition", async () => {
    const sql = await api.valueCondition(found.tab.sessionId, found.database, found.source, await found.raw());
    await copyText(sql, "condition");
  });
}

/** A table tab narrows its own rows; elsewhere the value's table opens filtered by it. */
async function filterByValue() {
  const found = sourced();
  if (!found) return;
  const { tab, source, database } = found;
  await attempt("Could not load the value", async () => {
    const match = { column: source.column, value: await found.raw() };
    const s = useStore.getState();
    if (tab.kind === "table" && tab.table === source.table) return s.filterRows(tab.id, match);
    s.openTableTab(database, source.table, "data", undefined, { preview: true, where: [match] });
  });
}

/** Table grids edit by their primary key, documents by the provenance row's single-column key. */
function editable(): boolean {
  const found = sourced();
  if (!found) return false;
  if (found.tab.kind === "table") return true;
  return found.tab.kind === "object" && found.source.row.length === 1;
}

const editValue = () => void window.dispatchEvent(new Event(EDIT_VALUE_EVENT));

type RuleFormKind = "reference" | "kind" | "style";

/** Opens the Project page with a rule form prefilled from the focused value's provenance (and, for a style, its text). */
export async function openRuleForm(kind: RuleFormKind) {
  const found = sourced();
  if (!found) return;
  const { tab, source, database } = found;
  await attempt("Could not prepare the rule", async () => {
    const draft = await api.ruleDraft(tab.sessionId, database, source);
    const form = kind === "reference" ? referenceFormFor(source, draft) : kind === "kind" ? kindFormFor(source, draft) : styleFormFor(source, draft, (await found.raw()) ?? "");
    useStore.getState().openProject({ sessionId: tab.sessionId, database, form });
  });
}

/** Rule entries for a value's context menu; `sourced` values only. */
export const RULE_MENU = [
  { label: "Add reference…", action: () => void openRuleForm("reference") },
  { label: "Kind…", action: () => void openRuleForm("kind") },
  { label: "Style…", action: () => void openRuleForm("style") },
];

export const canAddRule = () => !!sourced();

function valueCommands(scope: Scope, idPrefix: string): Command[] {
  return [
    { id: `${idPrefix}copyRaw`, title: "Copy value", scope, defaultBinding: "Y", when: () => !!focused(), run: copyRaw },
    { id: `${idPrefix}copyCondition`, title: "Copy value as SQL condition", scope, defaultBinding: "Shift+Y", when: () => !!sourced(), run: copyCondition },
    { id: `${idPrefix}edit`, title: "Edit value", scope, defaultBinding: "E", when: editable, run: editValue },
    { id: `${idPrefix}filter`, title: "Filter rows by value", scope, defaultBinding: "F", when: () => !!sourced(), run: filterByValue },
    { id: `${idPrefix}addReference`, title: "Add reference from this value…", scope, defaultBinding: "R", when: canAddRule, run: () => openRuleForm("reference") },
    { id: `${idPrefix}setKind`, title: "Set the kind of this column or meta key…", scope, when: canAddRule, run: () => openRuleForm("kind") },
    { id: `${idPrefix}setStyle`, title: "Add a value style for this value…", scope, when: canAddRule, run: () => openRuleForm("style") },
  ];
}

export const VALUE_COMMANDS: Command[] = valueCommands("table", "value.");
export const OBJECT_VALUE_COMMANDS: Command[] = valueCommands("object", "object.");
