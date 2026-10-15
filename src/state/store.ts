import { create } from "zustand";
import {
  accountOf,
  api,
  toAppError,
  type AppError,
  type Cell,
  type ColumnMeta,
  type ConnectionSpec,
  type DatabaseInfo,
  type ExecuteResponse,
  type Favorite,
  type FavoriteTarget,
  type LaunchEvent,
  type SessionInfo,
  type Settings,
  type TableInfo,
  type WpInfo,
} from "../api";

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
  /** Initial WHERE filter, e.g. when opened from a document's source link. */
  filter?: string;
}

export interface DocumentTab {
  id: string;
  kind: "document";
  sessionId: string;
  title: string;
  database: string;
  entity: string;
  entityId: string;
}

export interface OptionsTab {
  id: string;
  kind: "options";
  sessionId: string;
  title: string;
  database: string;
}

export type Tab = QueryTab | TableTab | DocumentTab | OptionsTab;

export interface Toast {
  id: number;
  kind: "error" | "info" | "success";
  message: string;
}

export type Dialog =
  | { type: "settings" }
  | { type: "connect"; spec?: ConnectionSpec; error?: AppError }
  | { type: "password"; spec: ConnectionSpec; error?: string; savePassword: boolean }
  | { type: "confirm"; title: string; message: string; confirmLabel: string; onConfirm: () => void }
  | { type: "prompt"; title: string; value: string; onSubmit: (value: string) => void };

export interface SchemaState {
  databases: DatabaseInfo[];
  tables: Record<string, TableInfo[]>;
}

export interface SelectedCell {
  column: ColumnMeta;
  cell: Cell;
}

interface State {
  settings: Settings | null;
  sessions: SessionInfo[];
  activeSessionId: string | null;
  tabs: Tab[];
  activeTabId: string | null;
  schema: Record<string, SchemaState>;
  favorites: Favorite[];
  toasts: Toast[];
  dialog: Dialog | null;
  selectedCell: SelectedCell | null;
  sidebarPanel: "tables" | "history" | "wordpress";
  /** WordPress detection per `session/database`; null = not WordPress. */
  wp: Record<string, WpInfo | null>;
}

interface Actions {
  loadSettings(): Promise<void>;
  saveSettings(settings: Settings): Promise<void>;
  handleLaunchEvents(events: LaunchEvent[]): Promise<void>;
  connect(args: { spec: ConnectionSpec; password?: string; requestId?: string; savePassword?: boolean }): Promise<boolean>;
  disconnect(sessionId: string): Promise<void>;
  setActiveSession(sessionId: string): void;
  switchDatabase(sessionId: string, database: string): Promise<void>;
  loadDatabases(sessionId: string): Promise<void>;
  loadTables(sessionId: string, database: string): Promise<void>;
  openQueryTab(sql?: string): void;
  openTableTab(database: string, table: string, view?: TableTab["view"], filter?: string): void;
  openDocument(entity: string, entityId: string, label?: string): void;
  openOptions(): void;
  loadWp(sessionId: string, database: string, refresh?: boolean): Promise<void>;
  updateTab(id: string, patch: Partial<QueryTab> | Partial<TableTab>): void;
  activateTab(id: string): void;
  closeTab(id: string): void;
  runQuery(tabId: string, sql: string, confirmed?: boolean): Promise<void>;
  cancelQuery(tabId: string): Promise<void>;
  loadFavorites(): Promise<void>;
  addFavorite(name: string, target: FavoriteTarget, global?: boolean): Promise<void>;
  addCurrentTabToFavorites(): void;
  renameFavorite(favorite: Favorite, name: string): Promise<void>;
  saveFavoriteScope(favorite: Favorite): Promise<void>;
  removeFavorite(id: string): Promise<void>;
  moveFavorite(id: string, beforeId: string | null): Promise<void>;
  openFavorite(favorite: Favorite): void;
  toast(kind: Toast["kind"], message: string): void;
  dismissToast(id: number): void;
  openDialog(dialog: Dialog): void;
  closeDialog(): void;
  selectCell(selected: SelectedCell | null): void;
  setSidebarPanel(panel: State["sidebarPanel"]): void;
}

export type Store = State & Actions;

const initialState: State = {
  settings: null,
  sessions: [],
  activeSessionId: null,
  tabs: [],
  activeTabId: null,
  schema: {},
  favorites: [],
  toasts: [],
  dialog: null,
  selectedCell: null,
  sidebarPanel: "tables",
  wp: {},
};

let nextId = 1;
const newId = (prefix: string) => `${prefix}-${nextId++}`;

export const useStore = create<Store>()((set, get) => {
  const activeSession = () => get().sessions.find((s) => s.id === get().activeSessionId) ?? null;

  const fail = (e: unknown, context: string) => {
    const err = toAppError(e);
    get().toast("error", `${context}: ${err.message}`);
    return err;
  };

  const queryTabCount = (sessionId: string) =>
    get().tabs.filter((t) => t.kind === "query" && t.sessionId === sessionId).length;

  return {
    ...initialState,

    async loadSettings() {
      try {
        set({ settings: await api.getSettings() });
      } catch (e) {
        fail(e, "Could not load settings");
      }
    },

    async saveSettings(settings) {
      try {
        set({ settings: await api.updateSettings(settings) });
        get().toast("success", "Settings saved");
      } catch (e) {
        fail(e, "Could not save settings");
      }
    },

    async handleLaunchEvents(events) {
      for (const event of events) {
        if (event.type === "error") {
          get().toast("error", event.message);
        } else {
          await get().connect({
            spec: event.spec,
            requestId: event.hasPassword ? event.requestId : undefined,
            savePassword: event.savePassword,
          });
        }
      }
    },

    async connect(args) {
      try {
        const session = await api.connect(args);
        set((s) => ({
          sessions: [...s.sessions.filter((x) => x.id !== session.id), session],
          activeSessionId: session.id,
          dialog: null,
        }));
        get().openQueryTab();
        void get().loadDatabases(session.id);
        void get().loadFavorites();
        get().toast("success", `Connected to ${session.displayName}`);
        return true;
      } catch (e) {
        const error = toAppError(e);
        if (error.kind === "password_required" || error.kind === "auth_failed") {
          set({
            dialog: {
              type: "password",
              spec: args.spec,
              error: error.kind === "auth_failed" ? error.message : undefined,
              savePassword: args.savePassword ?? true,
            },
          });
        } else {
          set({ dialog: { type: "connect", spec: args.spec, error } });
        }
        return false;
      }
    },

    async disconnect(sessionId) {
      try {
        await api.disconnect(sessionId);
      } catch (e) {
        fail(e, "Disconnect failed");
      }
      set((s) => {
        const sessions = s.sessions.filter((x) => x.id !== sessionId);
        const tabs = s.tabs.filter((t) => t.sessionId !== sessionId);
        const activeSessionId = s.activeSessionId === sessionId ? (sessions[0]?.id ?? null) : s.activeSessionId;
        const activeTabId = tabs.some((t) => t.id === s.activeTabId)
          ? s.activeTabId
          : (tabs.find((t) => t.sessionId === activeSessionId)?.id ?? null);
        const { [sessionId]: _removed, ...schema } = s.schema;
        return { sessions, tabs, activeSessionId, activeTabId, schema };
      });
      void get().loadFavorites();
    },

    setActiveSession(sessionId) {
      const tab = get().tabs.find((t) => t.sessionId === sessionId);
      set({ activeSessionId: sessionId, activeTabId: tab?.id ?? null, selectedCell: null });
      void get().loadFavorites();
    },

    async switchDatabase(sessionId, database) {
      try {
        await api.useDatabase(sessionId, database);
        set((s) => ({
          sessions: s.sessions.map((x) => (x.id === sessionId ? { ...x, database } : x)),
        }));
        void get().loadTables(sessionId, database);
        void get().loadWp(sessionId, database);
      } catch (e) {
        fail(e, `Cannot switch to ${database}`);
      }
    },

    async loadDatabases(sessionId) {
      try {
        const databases = await api.listDatabases(sessionId);
        set((s) => ({
          schema: {
            ...s.schema,
            [sessionId]: { databases, tables: s.schema[sessionId]?.tables ?? {} },
          },
        }));
        const current = get().sessions.find((x) => x.id === sessionId)?.database;
        if (current) {
          void get().loadTables(sessionId, current);
          void get().loadWp(sessionId, current);
        }
      } catch (e) {
        fail(e, "Could not list databases");
      }
    },

    async loadTables(sessionId, database) {
      try {
        const tables = await api.listTables(sessionId, database);
        set((s) => {
          const current = s.schema[sessionId] ?? { databases: [], tables: {} };
          return {
            schema: {
              ...s.schema,
              [sessionId]: { ...current, tables: { ...current.tables, [database]: tables } },
            },
          };
        });
      } catch (e) {
        fail(e, `Could not list tables of ${database}`);
      }
    },

    openQueryTab(sql = "") {
      const session = activeSession();
      if (!session) return;
      const tab: QueryTab = {
        id: newId("query"),
        kind: "query",
        sessionId: session.id,
        title: `Query ${queryTabCount(session.id) + 1}`,
        sql,
        running: false,
        activeResult: 0,
      };
      set((s) => ({ tabs: [...s.tabs, tab], activeTabId: tab.id, selectedCell: null }));
    },

    openTableTab(database, table, view = "data", filter) {
      const session = activeSession();
      if (!session) return;
      const existing = get().tabs.find(
        (t) =>
          t.kind === "table" && t.sessionId === session.id && t.database === database && t.table === table && t.filter === filter,
      );
      if (existing) {
        set((s) => ({
          activeTabId: existing.id,
          tabs: s.tabs.map((t) => (t.id === existing.id ? { ...t, view } : t)),
          selectedCell: null,
        }));
        return;
      }
      const tab: TableTab = {
        id: newId("table"),
        kind: "table",
        sessionId: session.id,
        title: filter ? `${table} (filtered)` : table,
        database,
        table,
        view,
        filter,
      };
      set((s) => ({ tabs: [...s.tabs, tab], activeTabId: tab.id, selectedCell: null }));
    },

    openDocument(entity, entityId, label) {
      const session = activeSession();
      if (!session?.database) return;
      const existing = get().tabs.find(
        (t) => t.kind === "document" && t.sessionId === session.id && t.entity === entity && t.entityId === entityId,
      );
      if (existing) {
        set({ activeTabId: existing.id, selectedCell: null });
        return;
      }
      const tab: DocumentTab = {
        id: newId("doc"),
        kind: "document",
        sessionId: session.id,
        title: `${label ?? entity} #${entityId}`,
        database: session.database,
        entity,
        entityId,
      };
      set((s) => ({ tabs: [...s.tabs, tab], activeTabId: tab.id, selectedCell: null }));
    },

    openOptions() {
      const session = activeSession();
      if (!session?.database) return;
      const existing = get().tabs.find((t) => t.kind === "options" && t.sessionId === session.id && t.database === session.database);
      if (existing) {
        set({ activeTabId: existing.id });
        return;
      }
      const tab: OptionsTab = { id: newId("options"), kind: "options", sessionId: session.id, title: "Options", database: session.database };
      set((s) => ({ tabs: [...s.tabs, tab], activeTabId: tab.id, selectedCell: null }));
    },

    async loadWp(sessionId, database, refresh = false) {
      try {
        const info = await api.wpDetect(sessionId, database, refresh);
        set((s) => ({ wp: { ...s.wp, [`${sessionId}/${database}`]: info } }));
      } catch (e) {
        fail(e, "WordPress detection failed");
      }
    },

    updateTab(id, patch) {
      set((s) => ({ tabs: s.tabs.map((t) => (t.id === id ? ({ ...t, ...patch } as Tab) : t)) }));
    },

    activateTab(id) {
      set({ activeTabId: id, selectedCell: null });
    },

    closeTab(id) {
      set((s) => {
        const index = s.tabs.findIndex((t) => t.id === id);
        if (index === -1) return s;
        const closing = s.tabs[index];
        const tabs = s.tabs.filter((t) => t.id !== id);
        let activeTabId = s.activeTabId;
        if (activeTabId === id) {
          const siblings = tabs.filter((t) => t.sessionId === closing.sessionId);
          activeTabId = (siblings[Math.min(index, siblings.length - 1)] ?? siblings[siblings.length - 1])?.id ?? null;
        }
        return { tabs, activeTabId, selectedCell: null };
      });
    },

    async runQuery(tabId, sql, confirmed = false) {
      const tab = get().tabs.find((t) => t.id === tabId);
      if (!tab || tab.kind !== "query" || !sql.trim()) return;
      get().updateTab(tabId, { running: true, error: undefined });
      try {
        const response = await api.execute(tab.sessionId, sql, confirmed);
        get().updateTab(tabId, {
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
          get().updateTab(tabId, { running: false });
          set({
            dialog: {
              type: "confirm",
              title: "Run destructive statement?",
              message: error.message.replace(/^This statement needs confirmation: /, ""),
              confirmLabel: "Run anyway",
              onConfirm: () => void get().runQuery(tabId, sql, true),
            },
          });
        } else {
          get().updateTab(tabId, { running: false, error });
        }
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

    async loadFavorites() {
      const session = activeSession();
      if (!session) {
        set({ favorites: [] });
        return;
      }
      try {
        set({ favorites: await api.listFavorites(accountOf(session.spec)) });
      } catch (e) {
        fail(e, "Could not load favorites");
      }
    },

    async addFavorite(name, target, global = false) {
      const session = activeSession();
      if (!session) return;
      try {
        await api.saveFavorite({ id: "", name, scope: global ? null : accountOf(session.spec), target });
        await get().loadFavorites();
      } catch (e) {
        fail(e, "Could not add favorite");
      }
    },

    addCurrentTabToFavorites() {
      const tab = get().tabs.find((t) => t.id === get().activeTabId);
      if (!tab) return;
      if (tab.kind === "document" || tab.kind === "options") {
        get().toast("info", "Only tables and queries can be pinned for now");
        return;
      }
      if (tab.kind === "table") {
        void get().addFavorite(tab.table, { kind: "table", database: tab.database, table: tab.table });
        return;
      }
      if (!tab.sql.trim()) {
        get().toast("info", "Write a query first, then add it to favorites");
        return;
      }
      set({
        dialog: {
          type: "prompt",
          title: "Name this query",
          value: tab.title,
          onSubmit: (name) => void get().addFavorite(name, { kind: "query", sql: tab.sql }),
        },
      });
    },

    async renameFavorite(favorite, name) {
      try {
        await api.saveFavorite({ ...favorite, name });
        await get().loadFavorites();
      } catch (e) {
        fail(e, "Could not rename favorite");
      }
    },

    async saveFavoriteScope(favorite) {
      const session = activeSession();
      if (!session) return;
      try {
        await api.saveFavorite({ ...favorite, scope: favorite.scope ? null : accountOf(session.spec) });
        await get().loadFavorites();
      } catch (e) {
        fail(e, "Could not update favorite");
      }
    },

    async removeFavorite(id) {
      try {
        await api.removeFavorite(id);
        set((s) => ({ favorites: s.favorites.filter((f) => f.id !== id) }));
      } catch (e) {
        fail(e, "Could not remove favorite");
      }
    },

    async moveFavorite(id, beforeId) {
      const ids = get().favorites.map((f) => f.id).filter((x) => x !== id);
      const at = beforeId ? ids.indexOf(beforeId) : ids.length;
      ids.splice(at === -1 ? ids.length : at, 0, id);
      const byId = new Map(get().favorites.map((f) => [f.id, f]));
      set({ favorites: ids.map((x) => byId.get(x)!).filter(Boolean) });
      try {
        await api.reorderFavorites(ids);
      } catch (e) {
        fail(e, "Could not reorder favorites");
      }
    },

    openFavorite(favorite) {
      if (favorite.target.kind === "table") {
        get().openTableTab(favorite.target.database, favorite.target.table);
      } else {
        get().openQueryTab(favorite.target.sql);
        const tab = get().tabs.find((t) => t.id === get().activeTabId);
        if (tab) get().updateTab(tab.id, { title: favorite.name });
      }
    },

    toast(kind, message) {
      const id = nextId++;
      set((s) => ({ toasts: [...s.toasts.slice(-4), { id, kind, message }] }));
      if (kind === "error") void api.logError(message).catch(() => {});
    },

    dismissToast(id) {
      set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
    },

    openDialog(dialog) {
      set({ dialog });
    },

    closeDialog() {
      set({ dialog: null });
    },

    selectCell(selectedCell) {
      set({ selectedCell });
    },

    setSidebarPanel(sidebarPanel) {
      set({ sidebarPanel });
    },
  };
});

/** Restores the initial state; for tests. */
export function resetStore(patch: Partial<State> = {}) {
  useStore.setState({ ...initialState, ...patch });
}

export const selectActiveSession = (s: Store) => s.sessions.find((x) => x.id === s.activeSessionId) ?? null;
export const selectActiveTab = (s: Store) => s.tabs.find((t) => t.id === s.activeTabId) ?? null;

/** WordPress info for the active session's current database (undefined = not probed yet). */
export const selectWp = (s: Store): WpInfo | null | undefined => {
  const session = selectActiveSession(s);
  return session?.database ? s.wp[`${session.id}/${session.database}`] : null;
};
