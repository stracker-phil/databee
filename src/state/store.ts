import { create } from "zustand";
import {
  accountOf,
  EMPTY_UI_STATE,
  api,
  toAppError,
  type AppError,
  type ChangeRequest,
  type PlannedChange,
  type Cell,
  type ColumnMeta,
  type ConnectionSpec,
  type DatabaseInfo,
  type ExecuteResponse,
  type Favorite,
  type FavoriteTarget,
  type Keybindings,
  type LaunchEvent,
  type PaneSize,
  type SessionInfo,
  type Settings,
  type TableInfo,
  type UiState,
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
  /** Preview tabs (single click in the sidebar) are replaced by the next preview; pinning keeps them. */
  preview?: boolean;
}

export type Tab = QueryTab | TableTab;

export type WpList =
  | "orders"
  | "subscriptions"
  | "customers"
  | "products"
  | "actions"
  | "sessions"
  | "content"
  | "settings"
  | "plugins";

export type WpRoute =
  | { screen: "dashboard" }
  | { screen: "list"; list: WpList; params?: Record<string, string>; selected?: { entity: string; id: string } }
  | { screen: "entity"; entity: string; id: string }
  | { screen: "compare"; entity: string; a: string; b: string }
  | { screen: "snapshots" }
  | { screen: "find"; value: string };

export type Mode = "wp" | "db";


export interface Toast {
  id: number;
  kind: "error" | "info" | "success";
  message: string;
  /** How many times the same message was reported while it was showing. */
  count: number;
}

export type Dialog =
  | { type: "settings" }
  | { type: "connect"; spec?: ConnectionSpec; error?: AppError }
  | { type: "password"; spec: ConnectionSpec; error?: string; savePassword: boolean }
  | { type: "confirm"; title: string; message: string; confirmLabel: string; onConfirm: () => void }
  | { type: "prompt"; title: string; value: string; onSubmit: (value: string) => void }
  | { type: "insertRow"; database: string; table: string; columns: import("../api").ColumnInfo[] };

export interface SchemaState {
  databases: DatabaseInfo[];
  tables: Record<string, TableInfo[]>;
}

export interface EntityMeta {
  key: string;
  /** Which list the entity belongs to, e.g. products for a product post. */
  parent?: WpList;
  title?: string;
  table: string;
  idColumn: string;
  id: string;
}

export type RecentNav =
  | { kind: "entity"; entity: string; id: string; title: string }
  | { kind: "table"; database: string; table: string };

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
  mode: Mode;
  /** The user picked a mode explicitly; don't switch automatically anymore. */
  modeChosen: boolean;
  wpNav: { stack: WpRoute[]; index: number };
  /** Bumped when data may have changed (applied edits, live refresh); views reload on change. */
  dataVersion: number;
  paletteOpen: boolean;
  /** Text the palette opens with, e.g. `t:` for tables. */
  paletteSeed: string;
  keybindings: Keybindings;
  /** Facts about the loaded entity page for breadcrumbs and the source-row command. */
  entityMeta: EntityMeta | null;
  /** Recently opened entities and tables, newest first. */
  recentNav: RecentNav[];
  /** Per-connection UI state (column widths, split width) by connection identity. */
  uiState: Record<string, UiState>;
  /** Planned edits waiting for review, and applied batches that can be undone. */
  pending: (PlannedChange & { id: number; sessionId: string })[];
  applied: { sessionId: string; changes: PlannedChange[] }[];
}

interface Actions {
  loadSettings(): Promise<void>;
  /** `quiet` skips the toast, for settings that apply as they change. */
  saveSettings(settings: Settings, opts?: { quiet?: boolean }): Promise<void>;
  handleLaunchEvents(events: LaunchEvent[]): Promise<void>;
  connect(args: { spec: ConnectionSpec; password?: string; requestId?: string; savePassword?: boolean }): Promise<boolean>;
  disconnect(sessionId: string): Promise<void>;
  setActiveSession(sessionId: string): void;
  switchDatabase(sessionId: string, database: string): Promise<void>;
  loadDatabases(sessionId: string): Promise<void>;
  loadTables(sessionId: string, database: string): Promise<void>;
  openQueryTab(sql?: string): void;
  openTableTab(database: string, table: string, view?: TableTab["view"], filter?: string, opts?: { preview?: boolean }): void;
  pinTab(id: string): void;
  setMode(mode: Mode): void;
  navigate(route: WpRoute): void;
  /** Changes the current route without adding a history entry (selection, filters). */
  replaceRoute(route: WpRoute): void;
  goBack(): void;
  goForward(): void;
  openEntity(entity: string, id: string): void;
  bumpData(): void;
  queueChange(change: ChangeRequest): Promise<boolean>;
  discardChange(id: number): void;
  discardAll(): void;
  applyChanges(): Promise<void>;
  undoLast(): Promise<void>;
  takeSnapshot(entity: string, id: string): Promise<void>;
  setPaletteOpen(open: boolean, seed?: string): void;
  saveKeybindings(bindings: Keybindings): Promise<void>;
  setEntityMeta(meta: EntityMeta | null): void;
  loadUiState(identity: string): Promise<void>;
  setTableWidths(identity: string, table: string, widths: Record<string, number>): void;
  /** Persists a dragged pane size for this connection. */
  setPaneSize(identity: string, pane: PaneSize, px: number): void;
  /** Activates the next (+1) or previous (-1) tab of the active session. */
  cycleTab(delta: number): void;
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
  mode: "db",
  modeChosen: false,
  wpNav: { stack: [{ screen: "dashboard" }], index: 0 },
  dataVersion: 0,
  paletteOpen: false,
  paletteSeed: "",
  keybindings: {},
  entityMeta: null,
  recentNav: [],
  uiState: {},
  pending: [],
  applied: [],
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
        const [settings, keybindings] = await Promise.all([api.getSettings(), api.getKeybindings().catch(() => ({}))]);
        set({ settings, keybindings });
      } catch (e) {
        fail(e, "Could not load settings");
      }
    },

    async saveSettings(settings, opts) {
      try {
        set({ settings: await api.updateSettings(settings) });
        if (!opts?.quiet) get().toast("success", "Settings saved");
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
        void get().loadUiState(session.identity);
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
      set({
        activeSessionId: sessionId,
        activeTabId: tab?.id ?? null,
        selectedCell: null,
        wpNav: { stack: [{ screen: "dashboard" }], index: 0 },
      });
      void get().loadFavorites();
    },

    async switchDatabase(sessionId, database) {
      try {
        await api.useDatabase(sessionId, database);
        set((s) => ({
          sessions: s.sessions.map((x) => (x.id === sessionId ? { ...x, database } : x)),
        }));
        set({ wpNav: { stack: [{ screen: "dashboard" }], index: 0 } });
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

    openTableTab(database, table, view = "data", filter, opts = {}) {
      const session = activeSession();
      if (!session) return;
      const existing = get().tabs.find(
        (t) =>
          t.kind === "table" && t.sessionId === session.id && t.database === database && t.table === table && t.filter === filter,
      );
      if (existing) {
        set((s) => ({
          activeTabId: existing.id,
          tabs: s.tabs.map((t) => (t.id === existing.id && t.kind === "table" ? { ...t, view, preview: opts.preview ? t.preview : false } : t)),
          selectedCell: null,
        }));
        return;
      }
      remember({ kind: "table", database, table });
      const tab: TableTab = {
        id: newId("table"),
        kind: "table",
        sessionId: session.id,
        title: filter ? `${table} (filtered)` : table,
        database,
        table,
        view,
        filter,
        preview: opts.preview ?? false,
      };
      set((s) => {
        // A preview tab takes the place of the previous preview tab of this session.
        const replaced = opts.preview ? s.tabs.find((t): t is TableTab => t.kind === "table" && !!t.preview && t.sessionId === session.id) : undefined;
        const kept = s.tabs.filter((t) => t.id !== replaced?.id);
        const at = replaced ? s.tabs.indexOf(replaced) : kept.length;
        return { tabs: [...kept.slice(0, at), tab, ...kept.slice(at)], activeTabId: tab.id, selectedCell: null };
      });
    },

    pinTab(id) {
      set((s) => ({ tabs: s.tabs.map((t) => (t.id === id && t.kind === "table" && t.preview ? { ...t, preview: false } : t)) }));
    },

    setMode(mode) {
      set({ mode, modeChosen: true });
    },

    navigate(route) {
      set((s) => {
        const current = s.wpNav.stack[s.wpNav.index];
        if (JSON.stringify(current) === JSON.stringify(route)) return { mode: "wp" as Mode };
        const stack = [...s.wpNav.stack.slice(0, s.wpNav.index + 1), route].slice(-100);
        return { mode: "wp" as Mode, wpNav: { stack, index: stack.length - 1 } };
      });
    },

    replaceRoute(route) {
      set((s) => {
        const stack = [...s.wpNav.stack];
        stack[s.wpNav.index] = route;
        return { wpNav: { ...s.wpNav, stack } };
      });
    },

    goBack() {
      set((s) => ({ wpNav: { ...s.wpNav, index: Math.max(0, s.wpNav.index - 1) } }));
    },

    goForward() {
      set((s) => ({ wpNav: { ...s.wpNav, index: Math.min(s.wpNav.stack.length - 1, s.wpNav.index + 1) } }));
    },

    openEntity(entity, id) {
      get().navigate({ screen: "entity", entity, id });
      remember({ kind: "entity", entity, id, title: `${entity.charAt(0).toUpperCase() + entity.slice(1)} #${id}` });
    },

    bumpData() {
      set((s) => ({ dataVersion: s.dataVersion + 1 }));
    },

    async queueChange(change) {
      const session = activeSession();
      if (!session) return false;
      try {
        const planned = await api.planChange(session.id, change);
        set((s) => ({ pending: [...s.pending, { ...planned, id: nextId++, sessionId: session.id }] }));
        return true;
      } catch (e) {
        fail(e, "Cannot prepare change");
        return false;
      }
    },

    discardChange(id) {
      set((s) => ({ pending: s.pending.filter((p) => p.id !== id) }));
    },

    discardAll() {
      set({ pending: [] });
    },

    async applyChanges() {
      const session = activeSession();
      const batch = get().pending.filter((p) => p.sessionId === session?.id);
      if (!session || batch.length === 0) return;
      try {
        await api.applyChanges(session.id, batch.flatMap((p) => p.statements));
        set((s) => ({
          pending: s.pending.filter((p) => !batch.includes(p)),
          applied: [...s.applied, { sessionId: session.id, changes: batch }].slice(-20),
        }));
        get().toast("success", `Applied ${batch.length} ${batch.length === 1 ? "change" : "changes"}`);
        get().bumpData();
      } catch (e) {
        fail(e, "Changes were not applied");
      }
    },

    async undoLast() {
      const session = activeSession();
      const last = [...get().applied].reverse().find((a) => a.sessionId === session?.id);
      if (!session || !last) return;
      const statements = [...last.changes].reverse().flatMap((c) => c.undo);
      if (statements.length === 0) {
        get().toast("info", "This change can't be undone automatically");
        return;
      }
      try {
        await api.applyChanges(session.id, statements);
        set((s) => ({ applied: s.applied.filter((a) => a !== last) }));
        get().toast("success", "Undone");
        get().bumpData();
      } catch (e) {
        fail(e, "Undo failed");
      }
    },

    async takeSnapshot(entity, id) {
      const session = activeSession();
      if (!session?.database) return;
      try {
        const info = await api.wpSnapshotTake(
          session.id,
          session.database,
          { entity, id, includeRelated: true, optionPattern: null, includeActions: true },
          `${entity} #${id}`,
        );
        get().toast("success", `Snapshot saved (${info.paths} values). Diff it under Snapshots.`);
      } catch (e) {
        fail(e, "Snapshot failed");
      }
    },

    setPaletteOpen(paletteOpen, seed = "") {
      set({ paletteOpen, paletteSeed: seed });
    },

    async saveKeybindings(bindings) {
      try {
        const saved = await api.setKeybindings(bindings);
        set({ keybindings: saved });
      } catch (e) {
        fail(e, "Could not save shortcuts");
      }
    },

    setEntityMeta(entityMeta) {
      set({ entityMeta });
    },

    async loadUiState(identity) {
      if (get().uiState[identity]) return;
      try {
        const ui = await api.getUiState(identity);
        set((s) => ({ uiState: { ...s.uiState, [identity]: ui } }));
      } catch (e) {
        fail(e, "Could not load view state");
      }
    },

    setTableWidths(identity, table, widths) {
      const current = get().uiState[identity] ?? EMPTY_UI_STATE;
      const ui: UiState = { ...current, tables: { ...current.tables, [table]: { widths } } };
      set((s) => ({ uiState: { ...s.uiState, [identity]: ui } }));
      persistUiState(identity, ui);
    },

    setPaneSize(identity, pane, px) {
      const current = get().uiState[identity] ?? EMPTY_UI_STATE;
      const ui: UiState = { ...current, [pane]: Math.round(px) };
      set((s) => ({ uiState: { ...s.uiState, [identity]: ui } }));
      persistUiState(identity, ui);
    },

    cycleTab(delta) {
      const s = get();
      const tabs = s.tabs.filter((t) => t.sessionId === s.activeSessionId);
      if (tabs.length < 2) return;
      const index = tabs.findIndex((t) => t.id === s.activeTabId);
      const next = tabs[(index + delta + tabs.length) % tabs.length];
      set({ activeTabId: next.id, selectedCell: null });
    },

    async loadWp(sessionId, database, refresh = false) {
      try {
        const info = await api.wpDetect(sessionId, database, refresh);
        set((s) => ({
          wp: { ...s.wp, [`${sessionId}/${database}`]: info },
          // WordPress databases open in WordPress mode until the user picks a mode.
          mode: info && !s.modeChosen && s.activeSessionId === sessionId ? "wp" : s.mode,
        }));
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
      if (kind === "error") void api.logError(message).catch(() => {});
      const same = get().toasts.find((t) => t.kind === kind && t.message === message);
      if (same) {
        set((s) => ({ toasts: s.toasts.map((t) => (t.id === same.id ? { ...t, count: t.count + 1 } : t)) }));
        return;
      }
      const id = nextId++;
      set((s) => ({ toasts: [...s.toasts.slice(-4), { id, kind, message, count: 1 }] }));
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

const uiTimers = new Map<string, ReturnType<typeof setTimeout>>();

/** Writes UI state at most twice a second per connection (drags fire constantly). */
function persistUiState(identity: string, ui: UiState) {
  clearTimeout(uiTimers.get(identity));
  uiTimers.set(
    identity,
    setTimeout(() => {
      uiTimers.delete(identity);
      void api.setUiState(identity, ui).catch(() => {});
    }, 500),
  );
}

/** Keeps the last 20 opened entities and tables for the palette's Recent section. */
function remember(item: RecentNav) {
  useStore.setState((s) => {
    const key = JSON.stringify(item.kind === "entity" ? [item.kind, item.entity, item.id] : [item.kind, item.database, item.table]);
    const rest = s.recentNav.filter((r) => JSON.stringify(r.kind === "entity" ? [r.kind, r.entity, r.id] : [r.kind, r.database, r.table]) !== key);
    return { recentNav: [item, ...rest].slice(0, 20) };
  });
}

/** Restores the initial state; for tests. */
export function resetStore(patch: Partial<State> = {}) {
  useStore.setState({ ...initialState, ...patch });
}

export const selectActiveSession = (s: Store) => s.sessions.find((x) => x.id === s.activeSessionId) ?? null;
export const selectWpRoute = (s: Store): WpRoute => s.wpNav.stack[s.wpNav.index] ?? { screen: "dashboard" };

export const selectActiveTab = (s: Store) => s.tabs.find((t) => t.id === s.activeTabId) ?? null;

/** WordPress info for the active session's current database (undefined = not probed yet). */
export const selectWp = (s: Store): WpInfo | null | undefined => {
  const session = selectActiveSession(s);
  return session?.database ? s.wp[`${session.id}/${session.database}`] : null;
};
