import type { Address } from "../../api";
import { objectColumn, type ColumnStack, type StackColumn, type ValueColumn } from "../../lib/columns";
import { selectActiveTab } from "../selectors";
import type { Slice, Store } from "../store";
import { restacked } from "./nav";
import { findTab, hasValueColumn, replaceTab, type ObjectTab, type Tab } from "./tabs";

export interface ColumnsSlice {
  /** Opens `target` as a column after column `from` of object tab `tabId`; the columns to its right close. */
  drill: (tabId: string, from: number, target: Address) => void;
  /** Opens a value after column `from` of an object tab, or as the right column of a table or query tab (`from` null). */
  openValueColumn: (tabId: string, from: number | null, column: ValueColumn) => void;
  /** The value column at `index` (null: a table or query tab's) shows `text`, the edited value. */
  updateValueColumn: (tabId: string, index: number | null, text: string) => void;
  /** Pops back to column `index`: it becomes active and the columns to its right close. */
  popToColumn: (tabId: string, index: number) => void;
  /** `Esc`: restores a maximized stack, else closes the rightmost column of the active tab, an object column or a value column. */
  closeRightmostColumn: () => void;
  /** Makes column `index` active without closing any; history does not record it. */
  setActiveColumn: (tabId: string, index: number) => void;
  /** `h`/`l`: the column left (-1) or right (+1) of the active one becomes active. */
  moveActiveColumn: (delta: number) => void;
  /** The shares of object tab `tabId`'s shown panes; null makes them equal. */
  setPaneWidths: (tabId: string, widths: number[] | null) => void;
  /** Shows only the active column of object tab `tabId`, or all panes again; a single column never stays maximized. */
  toggleMaximized: (tabId: string) => void;
}

/** The active column of an object tab, to tell whether an update moved it. */
const activeColumn = (tab: Tab | undefined) => (tab?.kind === "object" ? tab.columns[tab.active] : undefined);

/** A stack down to one column has nothing to maximize against. */
const unmaximizedAlone = (tab: Tab): Tab => (tab.kind === "object" && tab.maximized && tab.columns.length < 2 ? { ...tab, maximized: undefined } : tab);

/** The tab with `tab` in its place; a new active column leaves the tab's grid selection behind. */
function withTab(s: Store, tab: Tab): Partial<Store> {
  const moved = activeColumn(findTab(s, tab.id)) !== activeColumn(tab);
  const replaced = replaceTab(s, unmaximizedAlone(tab));
  return moved && s.selectedCell?.tabId === tab.id ? { ...replaced, selectedCell: null } : replaced;
}

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
        const tab = findTab(s, tabId, "object");
        return tab ? withTab(s, pushedAfter(tab, from, objectColumn(target))) : {};
      });
    },

    openValueColumn(tabId, from, column) {
      set((s) => {
        const tab = findTab(s, tabId);
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
        const tab = findTab(s, tabId);
        if (!tab) return {};
        if (index === null) return hasValueColumn(tab) ? withTab(s, { ...tab, valueColumn: { ...tab.valueColumn, text } }) : {};
        if (tab.kind !== "object") return {};
        const columns = tab.columns.map((c, i) => (i === index && c.kind === "value" ? { ...c, text } : c));
        return withTab(s, { ...tab, columns });
      });
    },

    popToColumn(tabId, index) {
      set((s) => {
        const tab = findTab(s, tabId, "object");
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
      if (tab.kind === "object" && tab.maximized) {
        set(withTab(s, { ...tab, maximized: undefined }));
        return;
      }
      if (tab.kind !== "object" || tab.columns.length < 2) return;
      const columns = tab.columns.slice(0, -1);
      set(withTab(s, restacked(tab, { columns, active: Math.min(tab.active, columns.length - 1) })));
    },

    setActiveColumn(tabId, index) {
      set((s) => {
        const tab = findTab(s, tabId, "object");
        if (!tab || tab.active === index || !isColumnIndex(tab, index)) return {};
        return withTab(s, restacked(tab, { columns: tab.columns, active: index }, { record: false }));
      });
    },

    moveActiveColumn(delta) {
      const tab = selectActiveTab(get());
      if (tab?.kind !== "object") return;
      get().setActiveColumn(tab.id, Math.max(0, Math.min(tab.columns.length - 1, tab.active + delta)));
    },

    setPaneWidths(tabId, widths) {
      set((s) => {
        const tab = findTab(s, tabId, "object");
        return tab ? replaceTab(s, { ...tab, paneWidths: widths ?? undefined }) : {};
      });
    },

    toggleMaximized(tabId) {
      set((s) => {
        const tab = findTab(s, tabId, "object");
        if (!tab || (!tab.maximized && tab.columns.length < 2)) return {};
        return replaceTab(s, { ...tab, maximized: tab.maximized ? undefined : true });
      });
    },
  }),
};
