/** Row commands read the store's selection, not the focus, so the palette and the context menu can run them. */
import { singleKey } from "../lib/cells";
import { address } from "../lib/address";
import { copyText } from "../lib/clipboard";
import { rowsToJson, rowsToTsv } from "../lib/format";
import { selectActiveTab, useStore } from "../state/store";
import { gridRowsOf } from "./gridRows";
import type { Handler } from "./catalog";

interface RowTarget {
  tabId: string;
  sessionId: string;
  database: string;
  table: string;
  keyColumn: string;
  key: string;
  column: string;
}

/** The selected row of the active table tab, when its table has a single-column key. */
export function rowTarget(): RowTarget | null {
  const s = useStore.getState();
  const tab = selectActiveTab(s);
  const selected = s.selectedCell;
  if (tab?.kind !== "table" || selected?.tabId !== tab.id) return null;
  const source = selected.value?.source;
  const key = source && source.table === tab.table ? singleKey(source.row) : null;
  if (!source || !key) return null;
  return { tabId: tab.id, sessionId: tab.sessionId, database: tab.database, table: tab.table, ...key, column: source.column };
}

const rowRequest = ({ database, table, keyColumn, key }: RowTarget) => ({ database, table, keyColumn, key });

/** The id of the pending plan that deletes the target row, or null. */
function pendingDelete(target: RowTarget): number | null {
  const plan = useStore
    .getState()
    .pending.find(
      (p) =>
        p.sessionId === target.sessionId &&
        p.rows.some((r) => r.effect.kind === "delete" && r.database === target.database && r.table === target.table && r.keyColumn === target.keyColumn && r.key === target.key),
    );
  return plan?.id ?? null;
}

/** Deleting a row already queued for deletion takes the plan back. */
function toggleDelete() {
  const target = rowTarget();
  if (!target) return;
  const planned = pendingDelete(target);
  const s = useStore.getState();
  if (planned !== null) {
    void s.discardChange(planned);
    return;
  }
  void s.queueChange({ type: "deleteRow", ...rowRequest(target) });
}

function duplicate() {
  const target = rowTarget();
  if (!target) return;
  void useStore.getState().queueChange({ type: "duplicateRow", ...rowRequest(target) });
}

function setNull() {
  const target = rowTarget();
  if (!target) return;
  const s = useStore.getState();
  s.pinTab(target.tabId);
  void s.queueChange({ type: "setCell", ...rowRequest(target), column: target.column, value: null });
}

function openObject() {
  const target = rowTarget();
  if (!target) return;
  const s = useStore.getState();
  const slug = s.sessions.find((x) => x.id === target.sessionId)?.slug;
  if (slug === undefined) return;
  s.openObject(address(slug, target.database, { kind: "row", table: target.table, key: target.key }));
}

/** The selected cell's grid in the active tab. */
function selectedGrid() {
  const s = useStore.getState();
  const tab = selectActiveTab(s);
  const selected = s.selectedCell;
  if (!tab || selected?.tabId !== tab.id) return null;
  const grid = gridRowsOf(selected.source);
  return grid && grid.rows[selected.row] ? { ...grid, row: selected.row } : null;
}

function copyRows(format: "tsv" | "json", which: "row" | "all") {
  const grid = selectedGrid();
  if (!grid) return;
  const rows = which === "row" ? [grid.rows[grid.row]] : grid.rows;
  const text = format === "tsv" ? rowsToTsv(grid.names, rows) : rowsToJson(grid.names, rows);
  const what = which === "row" ? "row" : `${rows.length} rows`;
  void copyText(text, format === "tsv" ? what : `${what} as JSON`);
}

const hasRow = () => rowTarget() !== null;
const hasGrid = () => selectedGrid() !== null;

export const ROW_COMMANDS: Handler[] = [
  { id: "row.delete", when: hasRow, run: toggleDelete },
  { id: "row.duplicate", when: hasRow, run: duplicate },
  { id: "value.setNull", when: hasRow, run: setNull },
  { id: "row.openObject", when: hasRow, run: openObject },
  { id: "row.copyTsv", when: hasGrid, run: () => copyRows("tsv", "row") },
  { id: "row.copyJson", when: hasGrid, run: () => copyRows("json", "row") },
  { id: "grid.copyAllTsv", when: hasGrid, run: () => copyRows("tsv", "all") },
  { id: "grid.copyAllJson", when: hasGrid, run: () => copyRows("json", "all") },
];
