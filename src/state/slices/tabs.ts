import { api, toAppError, type AppError, type Cell, type ColumnEquals, type ColumnMeta, type ExecuteResponse } from "../../api";
import type { CellSource } from "../../lib/cells";
import { fail } from "../requests";
import { selectActiveSession } from "../selectors";
import type { Slice, Store } from "../store";
import { nextActiveSessionId } from "./session";
import { remembered } from "./ui";

export interface QueryTab {
  id: string;
  kind: "query";
  sessionId: string;
  title: string;
  sql: string;
  running: boolean;
  response?: ExecuteResponse;
  error?: AppError;
  activeResult: number;
}

export interface TableTab {
  id: string;
  kind: "table";
  sessionId: string;
  title: string;
  database: string;
  table: string;
  view: "data" | "structure";
  /** Initial free-text WHERE filter, as typed. */
  filter?: string;
  /** Exact column matches Rust turns into SQL, e.g. when opened from a document's source link. */
  where: ColumnEquals[];
  /** Preview tabs (single click in the sidebar) are replaced by the next preview; pinning keeps them. */
  preview?: boolean;
}

export type Tab = QueryTab | TableTab;

export interface SelectedCell {
  /** The tab whose grid holds the cell; closing, leaving or reloading it clears the selection. */
  tabId: string;
  row: number;
  col: number;
  column: ColumnMeta;
  cell: Cell;
  /** Lets the Inspector fetch a `longText` cell in full. */
  source: CellSource;
}

export interface TabsSlice {
  tabs: Tab[];
  activeTabId: string | null;
  selectedCell: SelectedCell | null;
  /** Query tabs opened per session, for unique default titles. */
  queryCounter: Record<string, number>;
  openQueryTab(sql?: string, title?: string): void;
  /** An open tab of the same table is activated; `view` and `preview: false` (pin) apply to it only when given. */
  openTableTab(database: string, table: string, view?: TableTab["view"], filter?: string, opts?: { preview?: boolean; where?: ColumnEquals[] }): void;
  /** Switches to the Database view and opens `table` of the active database filtered by `where`. */
  openInDatabase(table: string, where?: ColumnEquals[]): void;
  /** Switches to the Database view and opens the rows of `table` where `column` equals `value`, in the preview tab. */
  openRow(table: string, column: string, value: string | null): void;
  pinTab(id: string): void;
  activateTab(id: string): void;
  closeTab(id: string): void;
  /** Activates the next (+1) or previous (-1) tab of the active session. */
  cycleTab(delta: number): void;
  updateQueryTab(id: string, patch: Partial<QueryTab>): void;
  updateTableTab(id: string, patch: Partial<TableTab>): void;
  /** Selects a cell of the active tab's grid. */
  selectCell(selected: Omit<SelectedCell, "tabId"> | null): void;
  runQuery(tabId: string, sql: string, confirmed?: boolean): Promise<void>;
  cancelQuery(tabId: string): Promise<void>;
}

let nextTabId = 1;

export const resetSelection = () => ({ selectedCell: null });

/** The selection, unless it belongs to `tabId`. */
function selectionOutside(s: Store, tabId: string | null) {
  return s.selectedCell?.tabId === tabId ? null : s.selectedCell;
}

/** Activates `tabId`; the selection belongs to the tab being left. */
export function activate(s: Store, tabId: string | null) {
  return { activeTabId: tabId, selectedCell: tabId === s.activeTabId ? s.selectedCell : selectionOutside(s, s.activeTabId) };
}

function sameWhere(a: ColumnEquals[], b: ColumnEquals[]): boolean {
  return a.length === b.length && a.every((x, i) => x.column === b[i].column && x.value === b[i].value);
}

export const tabsSlice: Slice<TabsSlice> = {
  create: (set, get) => ({
    tabs: [],
    activeTabId: null,
    queryCounter: {},
    ...resetSelection(),

    openQueryTab(sql = "", title) {
      const session = selectActiveSession(get());
      if (!session) return;
      set((s) => {
        // Only untitled tabs take a number.
        const previous = s.queryCounter[session.id] ?? 0;
        const count = title ? previous : previous + 1;
        const tab: QueryTab = {
          id: `query-${nextTabId++}`,
          kind: "query",
          sessionId: session.id,
          title: title ?? `Query ${count}`,
          sql,
          running: false,
          activeResult: 0,
        };
        return { tabs: [...s.tabs, tab], queryCounter: { ...s.queryCounter, [session.id]: count }, ...activate(s, tab.id) };
      });
    },

    openTableTab(database, table, view, filter, opts = {}) {
      const session = selectActiveSession(get());
      if (!session) return;
      const where = opts.where ?? [];
      set((s) => remembered(s, { kind: "table", database, table }));
      const existing = get().tabs.find(
        (t): t is TableTab =>
          t.kind === "table" &&
          t.sessionId === session.id &&
          t.database === database &&
          t.table === table &&
          t.filter === filter &&
          sameWhere(t.where, where),
      );
      if (existing) {
        const updated: TableTab = { ...existing, view: view ?? existing.view, preview: existing.preview && opts.preview !== false };
        set((s) => ({ tabs: s.tabs.map((t) => (t.id === existing.id ? updated : t)), ...activate(s, existing.id) }));
        return;
      }
      const tab: TableTab = {
        id: `table-${nextTabId++}`,
        kind: "table",
        sessionId: session.id,
        title: filter || where.length > 0 ? `${table} (filtered)` : table,
        database,
        table,
        view: view ?? "data",
        filter,
        where,
        preview: opts.preview ?? false,
      };
      set((s) => {
        // A preview tab takes the place of the previous preview tab of this session.
        const replaced = opts.preview ? s.tabs.find((t): t is TableTab => t.kind === "table" && !!t.preview && t.sessionId === session.id) : undefined;
        const kept = s.tabs.filter((t) => t.id !== replaced?.id);
        const at = replaced ? s.tabs.indexOf(replaced) : kept.length;
        return { tabs: [...kept.slice(0, at), tab, ...kept.slice(at)], ...activate(s, tab.id) };
      });
    },

    openInDatabase(table, where) {
      const database = selectActiveSession(get())?.database;
      if (!database) return;
      get().setMode("db");
      get().openTableTab(database, table, "data", undefined, { where });
    },

    openRow(table, column, value) {
      const database = selectActiveSession(get())?.database;
      if (!database) return;
      get().setMode("db");
      get().openTableTab(database, table, "data", undefined, { preview: true, where: [{ column, value }] });
    },

    pinTab(id) {
      set((s) => ({ tabs: s.tabs.map((t) => (t.id === id && t.kind === "table" && t.preview ? { ...t, preview: false } : t)) }));
    },

    activateTab(id) {
      set((s) => activate(s, id));
    },

    closeTab(id) {
      set((s) => {
        const closing = s.tabs.find((t) => t.id === id);
        if (!closing) return {};
        const tabs = s.tabs.filter((t) => t.id !== id);
        const selectedCell = selectionOutside(s, id);
        if (s.activeTabId !== id) return { tabs, selectedCell };
        const sessionTabs = s.tabs.filter((t) => t.sessionId === closing.sessionId);
        const index = sessionTabs.indexOf(closing);
        const siblings = sessionTabs.filter((t) => t !== closing);
        const neighbour = siblings[Math.min(index, siblings.length - 1)];
        return { tabs, activeTabId: neighbour?.id ?? null, selectedCell };
      });
    },

    cycleTab(delta) {
      const s = get();
      const tabs = s.tabs.filter((t) => t.sessionId === s.activeSessionId);
      if (tabs.length < 2) return;
      const index = tabs.findIndex((t) => t.id === s.activeTabId);
      const next = tabs[(index + delta + tabs.length) % tabs.length];
      set(activate(s, next.id));
    },

    updateQueryTab(id, patch) {
      set((s) => ({ tabs: s.tabs.map((t) => (t.id === id && t.kind === "query" ? { ...t, ...patch } : t)) }));
    },

    updateTableTab(id, patch) {
      set((s) => ({ tabs: s.tabs.map((t) => (t.id === id && t.kind === "table" ? { ...t, ...patch } : t)) }));
    },

    selectCell(selected) {
      set((s) => ({ selectedCell: selected && s.activeTabId ? { ...selected, tabId: s.activeTabId } : null }));
    },

    async runQuery(tabId, sql, confirmed = false) {
      const tab = get().tabs.find((t) => t.id === tabId);
      // `running` is set synchronously below, so a second run cannot start before the first replies.
      if (!tab || tab.kind !== "query" || tab.running || !sql.trim()) return;
      get().updateQueryTab(tabId, { running: true, error: undefined });
      try {
        const response = await api.execute(tab.sessionId, sql, confirmed);
        get().updateQueryTab(tabId, {
          running: false,
          response,
          error: undefined,
          activeResult: Math.max(0, response.results.length - 1),
        });
        const session = get().sessions.find((s) => s.id === tab.sessionId);
        if (session && response.database !== session.database) {
          set((s) => ({
            sessions: s.sessions.map((x) => (x.id === tab.sessionId ? { ...x, database: response.database } : x)),
          }));
        }
        // Schema may have changed (CREATE/DROP/ALTER); table lists are cheap to reload.
        if (response.database && response.results.some((r) => !r.returnsRows)) {
          void get().loadTables(tab.sessionId, response.database);
        }
      } catch (e) {
        const error = toAppError(e);
        if (error.kind === "confirmation_required") {
          get().updateQueryTab(tabId, { running: false });
          set({
            dialog: {
              type: "confirm",
              title: "Run destructive statement?",
              message: error.reason ?? error.message,
              confirmLabel: "Run anyway",
              onConfirm: () => void get().runQuery(tabId, sql, true),
            },
          });
        } else {
          get().updateQueryTab(tabId, { running: false, error });
          if (error.sessionReset) {
            get().toast("info", "Session reset after timeout: temporary tables, variables and open transactions are gone");
          }
        }
      } finally {
        set((s) => ({ historyVersion: s.historyVersion + 1 }));
      }
    },

    async cancelQuery(tabId) {
      const tab = get().tabs.find((t) => t.id === tabId);
      if (!tab) return;
      try {
        await api.cancelQuery(tab.sessionId);
      } catch (e) {
        fail(e, "Cancel failed");
      }
    },
  }),

  sessionClosed(sessionId, s) {
    const tabs = s.tabs.filter((t) => t.sessionId !== sessionId);
    const activeSessionId = nextActiveSessionId(s, sessionId);
    const activeTabId = tabs.some((t) => t.id === s.activeTabId)
      ? s.activeTabId
      : (tabs.find((t) => t.sessionId === activeSessionId)?.id ?? null);
    const selectionKept = tabs.some((t) => t.id === s.selectedCell?.tabId);
    const { [sessionId]: _counter, ...queryCounter } = s.queryCounter;
    return { tabs, activeTabId, queryCounter, selectedCell: selectionKept ? s.selectedCell : null };
  },
};
