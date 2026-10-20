import type { Address } from "../../api";
import { fetchStoredValue } from "../../lib/cells";
import { objectColumn, type ColumnStack, type StackColumn, type ValueColumn } from "../../lib/columns";
import { fail } from "../requests";
import { selectActiveTab } from "../selectors";
import type { Slice, Store } from "../store";
import { blockedUnlessLoaded, restacked } from "./nav";
import { findTab, hasValueColumn, replaceTab, type ObjectTab, type Tab } from "./tabs";

export interface ColumnsSlice {
  /** Opens `target` as a column after column `from` of object tab `tabId`; the columns to its right close. */
  drill: (tabId: string, from: number, target: Address) => void;
  /** Opens a value after column `from` of an object tab, or as the right column of a table or query tab (`from` null). */
  openValueColumn: (tabId: string, from: number | null, column: ValueColumn) => void;
  /** The value column at `index` (null: a table or query tab's) shows `text`, the edited value that plan `planId` queued. */
  updateValueColumn: (tabId: string, index: number | null, text: string, planId: number) => void;
  /**
   * Reads the value of the value column at `index` (null: a table or query tab's) again by its row key; a row that
   * is gone marks it `missing`. While an edit it queued is pending, the edited text stays.
   */
  reloadValueColumn: (tabId: string, index: number | null) => Promise<void>;
  /** Pops back to column `index`: it becomes active and the columns to its right close. */
  popToColumn: (tabId: string, index: number) => void;
  /** `Esc`: restores a maximized stack, else closes the rightmost column of the active tab, an object column or a value column. */
  closeRightmostColumn: () => void;
  /** Closes the active column of the active object tab and the columns drilled from it (those to its right); the first column stays. */
  closeActiveColumn: () => void;
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

/** The value column at `index` of an object tab, or a table or query tab's (`index` null). */
function valueColumnOf(tab: Tab, index: number | null): ValueColumn | null {
  if (index === null) return hasValueColumn(tab) ? tab.valueColumn : null;
  const column = tab.kind === "object" ? tab.columns[index] : undefined;
  return column?.kind === "value" ? column : null;
}

/** `tab` with `next` in place of value column `index`; null when the tab no longer holds `previous` there. */
function withValueColumn(tab: Tab, index: number | null, previous: ValueColumn, next: ValueColumn): Tab | null {
  if (valueColumnOf(tab, index) !== previous) return null;
  if (index === null) return { ...tab, valueColumn: next } as Tab;
  return tab.kind === "object" ? { ...tab, columns: tab.columns.map((c, i) => (i === index ? next : c)) } : null;
}

/** `tab` with `column` after column `from`, active; history records the address left. */
function pushedAfter(tab: ObjectTab, from: number, column: StackColumn): ObjectTab {
  const stack: ColumnStack = { columns: [...tab.columns.slice(0, from + 1), column], active: from + 1 };
  return restacked(tab, stack);
}

export const columnsSlice: Slice<ColumnsSlice> = {
  create: (set, get) => ({
    drill(tabId, from, target) {
      const shown = findTab(get(), tabId, "object");
      if (target.target.kind !== "row" || (shown && blockedUnlessLoaded(get, shown.sessionId, target))) return;
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
          return withTab(s, pushedAfter(tab, from, { ...column, version: s.dataVersion }));
        }
        if (tab.kind === "table" || tab.kind === "query") return withTab(s, { ...tab, valueColumn: { ...column, version: s.dataVersion } });
        return {};
      });
    },

    updateValueColumn(tabId, index, text, planId) {
      set((s) => {
        const tab = findTab(s, tabId);
        const column = tab && valueColumnOf(tab, index);
        const updated = column && withValueColumn(tab, index, column, { ...column, text, queued: [...(column.queued ?? []), planId] });
        return updated ? withTab(s, updated) : {};
      });
    },

    async reloadValueColumn(tabId, index) {
      const s = get();
      const tab = findTab(s, tabId);
      const column = tab && valueColumnOf(tab, index);
      const source = column?.value.source;
      // A lost session keeps the value it read; the reload that asked for it says why.
      if (!tab || !column || !source || !column.database || s.health[tab.sessionId]) return;
      const version = s.dataVersion;
      const write = (patch: Partial<ValueColumn>) =>
        set((s) => {
          const current = findTab(s, tabId);
          const updated = current && withValueColumn(current, index, column, { ...column, ...patch, version });
          return updated ? replaceTab(s, updated) : {};
        });
      const pendingIds = new Set(s.pending.filter((p) => p.sessionId === tab.sessionId).map((p) => p.id));
      if (column.queued?.some((id) => pendingIds.has(id))) return write({});
      try {
        const stored = await fetchStoredValue(tab.sessionId, column.database, source);
        write(stored ? { text: stored.text, value: { ...column.value, raw: stored.cell }, queued: undefined, missing: undefined } : { queued: undefined, missing: true });
      } catch (e) {
        write({});
        fail(e, "Could not reload the value");
      }
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

    closeActiveColumn() {
      const tab = selectActiveTab(get());
      if (tab?.kind === "object" && tab.active > 0) get().popToColumn(tab.id, tab.active - 1);
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
