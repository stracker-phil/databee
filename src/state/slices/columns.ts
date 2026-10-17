import type { Address } from "../../api";
import { objectColumn, type ColumnStack, type StackColumn, type ValueColumn } from "../../lib/columns";
import { selectActiveTab } from "../selectors";
import type { Slice, Store } from "../store";
import { restacked } from "./nav";
import type { ObjectTab, Tab } from "./tabs";

export interface ColumnsSlice {
  /** Opens `target` as a column after column `from` of object tab `tabId`; the columns to its right close. */
  drill(tabId: string, from: number, target: Address): void;
  /** Opens a value after column `from` of an object tab, or as the right column of a table or query tab (`from` null). */
  openValueColumn(tabId: string, from: number | null, column: ValueColumn): void;
  /** The value column at `index` (null: a table or query tab's) shows `text`, the edited value. */
  updateValueColumn(tabId: string, index: number | null, text: string): void;
  /** Pops back to column `index`: it becomes active and the columns to its right close. */
  popToColumn(tabId: string, index: number): void;
  /** `Esc`: closes the rightmost column of the active tab, an object column or a value column. */
  closeRightmostColumn(): void;
  /** Makes column `index` active without closing any; history does not record it. */
  setActiveColumn(tabId: string, index: number): void;
  /** `h`/`l`: the column left (-1) or right (+1) of the active one becomes active. */
  moveActiveColumn(delta: number): void;
}

const findObjectTab = (s: Store, tabId: string) => s.tabs.find((t): t is ObjectTab => t.id === tabId && t.kind === "object");

/** The active column of an object tab, to tell whether an update moved it. */
const activeColumn = (tab: Tab | undefined) => (tab?.kind === "object" ? tab.columns[tab.active] : undefined);

/** The tab with `tab` in its place; a new active column leaves the tab's grid selection behind. */
function withTab(s: Store, tab: Tab): Partial<Store> {
  const previous = s.tabs.find((t) => t.id === tab.id);
  const tabs = s.tabs.map((t) => (t.id === tab.id ? tab : t));
  const moved = activeColumn(previous) !== activeColumn(tab);
  return moved && s.selectedCell?.tabId === tab.id ? { tabs, selectedCell: null } : { tabs };
}

const hasValueColumn = (tab: Tab): tab is Extract<Tab, { kind: "table" | "query" }> & { valueColumn: ValueColumn } =>
  (tab.kind === "table" || tab.kind === "query") && !!tab.valueColumn;

const isColumnIndex = (tab: ObjectTab, index: number) => index >= 0 && index < tab.columns.length;

/** `tab` with `column` after column `from`, active; history records the address left. */
function pushedAfter(tab: ObjectTab, from: number, column: StackColumn): ObjectTab {
  const stack: ColumnStack = { columns: [...tab.columns.slice(0, from + 1), column], active: from + 1 };
  return restacked(tab, stack);
}

export const columnsSlice: Slice<ColumnsSlice> = {
  create: (set, get) => ({
    drill(tabId, from, target) {
      if (target.target.kind !== "row") return;
      set((s) => {
        const tab = findObjectTab(s, tabId);
        return tab ? withTab(s, pushedAfter(tab, from, objectColumn(target))) : {};
      });
    },

    openValueColumn(tabId, from, column) {
      set((s) => {
        const tab = s.tabs.find((t) => t.id === tabId);
        if (!tab) return {};
        if (tab.kind === "object") {
          if (from === null) return {};
          return withTab(s, pushedAfter(tab, from, column));
        }
        if (tab.kind === "table" || tab.kind === "query") return withTab(s, { ...tab, valueColumn: column });
        return {};
      });
    },

    updateValueColumn(tabId, index, text) {
      set((s) => {
        const tab = s.tabs.find((t) => t.id === tabId);
        if (!tab) return {};
        if (index === null) return hasValueColumn(tab) ? withTab(s, { ...tab, valueColumn: { ...tab.valueColumn, text } }) : {};
        if (tab.kind !== "object") return {};
        const columns = tab.columns.map((c, i) => (i === index && c.kind === "value" ? { ...c, text } : c));
        return withTab(s, { ...tab, columns });
      });
    },

    popToColumn(tabId, index) {
      set((s) => {
        const tab = findObjectTab(s, tabId);
        if (!tab || !isColumnIndex(tab, index)) return {};
        return withTab(s, restacked(tab, { columns: tab.columns.slice(0, index + 1), active: index }));
      });
    },

    closeRightmostColumn() {
      const s = get();
      const tab = selectActiveTab(s);
      if (!tab) return;
      if (hasValueColumn(tab)) {
        set(withTab(s, { ...tab, valueColumn: undefined }));
        return;
      }
      if (tab.kind !== "object" || tab.columns.length < 2) return;
      const columns = tab.columns.slice(0, -1);
      set(withTab(s, restacked(tab, { columns, active: Math.min(tab.active, columns.length - 1) })));
    },

    setActiveColumn(tabId, index) {
      set((s) => {
        const tab = findObjectTab(s, tabId);
        if (!tab || tab.active === index || !isColumnIndex(tab, index)) return {};
        return withTab(s, restacked(tab, { columns: tab.columns, active: index }, { record: false }));
      });
    },

    moveActiveColumn(delta) {
      const tab = selectActiveTab(get());
      if (tab?.kind !== "object") return;
      get().setActiveColumn(tab.id, Math.max(0, Math.min(tab.columns.length - 1, tab.active + delta)));
    },
  }),
};
