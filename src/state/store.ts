import { create } from "zustand";
import { setOverrides } from "../commands/registry";
import {
  accountOf,
  EMPTY_UI_STATE,
  api,
  toAppError,
  type AppError,
  type ChangeRequest,
  type PlannedChange,
  type Cell,
  type ColumnInfo,
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
  | "users"
  | "products"
  | "actions"
  | "sessions"
  | "content"
  | "settings";

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
  /** `page`: the Settings page shown (SETTINGS_PAGES ids); the first one when absent. */
  | { type: "settings"; page?: string }
  | { type: "connect"; spec?: ConnectionSpec; error?: AppError }
  | { type: "password"; spec: ConnectionSpec; error?: string; savePassword: boolean }
  | { type: "confirm"; title: string; message: string; confirmLabel: string; onConfirm: () => void }
  | { type: "prompt"; title: string; value: string; onSubmit: (value: string) => void }
  | { type: "insertRow"; database: string; table: string; columns: ColumnInfo[] };

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
  /** The tab whose grid holds the cell; closing, leaving or reloading it clears the selection. */
  tabId: string;
  row: number;
  col: number;
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
  sidebarPanel: "tables" | "history";
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
  /** An apply or undo is in flight; both refuse to start another. */
  applying: boolean;
  /** Query tabs opened per session, for unique default titles. */
  queryCounter: Record<string, number>;
}

interface Actions {
  loadSettings(): Promise<void>;
  /** Replaces the Stage API token (Rust saves it and restarts the API). */
  newStageToken(): Promise<void>;
  /** `quiet` skips the toast, for settings that apply as they change. */
  saveSettings(settings: Settings, opts?: { quiet?: boolean }): Promise<void>;
  handleLaunchEvents(events: LaunchEvent[]): Promise<void>;
  connect(args: { spec: ConnectionSpec; password?: string; requestId?: string; savePassword?: boolean }): Promise<boolean>;
  disconnect(sessionId: string): Promise<void>;
  setActiveSession(sessionId: string): void;
  switchDatabase(sessionId: string, database: string): Promise<void>;
  loadDatabases(sessionId: string): Promise<void>;
  loadTables(sessionId: string, database: string): Promise<void>;
  openQueryTab(sql?: string, title?: string): void;
  /** An open tab of the same table is activated; `view` and `preview: false` (pin) apply to it only when given. */
  openTableTab(database: string, table: string, view?: TableTab["view"], filter?: string, opts?: { preview?: boolean }): void;
  /** Switches to the Database view and opens `table` of the active database filtered by `where`. */
  openInDatabase(table: string, where?: string): void;
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
  updateQueryTab(id: string, patch: Partial<QueryTab>): void;
  updateTableTab(id: string, patch: Partial<TableTab>): void;
  activateTab(id: string): void;
  closeTab(id: string): void;
  runQuery(tabId: string, sql: string, confirmed?: boolean): Promise<void>;
  cancelQuery(tabId: string): Promise<void>;
  loadFavorites(): Promise<void>;
  addFavorite(name: string, target: FavoriteTarget, global?: boolean): Promise<void>;
  addCurrentTabToFavorites(): void;
  renameFavorite(favorite: Favorite, name: string): Promise<void>;
  toggleFavoriteScope(favorite: Favorite): Promise<void>;
  removeFavorite(id: string): Promise<void>;
  moveFavorite(id: string, beforeId: string | null): Promise<void>;
  openFavorite(favorite: Favorite): void;
  toast(kind: Toast["kind"], message: string): void;
  dismissToast(id: number): void;
  openDialog(dialog: Dialog): void;
  closeDialog(): void;
  /** Selects a cell of the active tab's grid. */
  selectCell(selected: Omit<SelectedCell, "tabId"> | null): void;
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
  applying: false,
  queryCounter: {},
};

const HOME_NAV: State["wpNav"] = { stack: [{ screen: "dashboard" }], index: 0 };

let nextTabId = 1;
let nextPendingId = 1;
let nextToastId = 1;

/** Shows an error toast (which also logs it) and returns the error. */
function fail(e: unknown, context: string) {
  const err = toAppError(e);
  useStore.getState().toast("error", `${context}: ${err.message}`);
  return err;
}

/** The selection, unless it belongs to `tabId`. */
function selectionOutside(s: State, tabId: string | null) {
  return s.selectedCell?.tabId === tabId ? null : s.selectedCell;
}

/** Activates `tabId`; the selection belongs to the tab being left. */
function activate(s: State, tabId: string | null) {
  return { activeTabId: tabId, selectedCell: tabId === s.activeTabId ? s.selectedCell : selectionOutside(s, s.activeTabId) };
}

/** Request tokens: only the newest request per key may write its reply. */
const latestRequest = new Map<string, number>();
let nextRequestToken = 1;

function beginRequest(key: string) {
  const token = nextRequestToken++;
  latestRequest.set(key, token);
  return token;
}

const isLatest = (key: string, token: number) => latestRequest.get(key) === token;

/** Route identity that does not depend on property order. */
function routeKey(route: WpRoute): string {
  const sorted = (value: unknown): unknown =>
    value && typeof value === "object"
      ? Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => [k, sorted(v)]))
      : value;
  return JSON.stringify(sorted(route));
}

export const useStore = create<Store>()((set, get) => {
  /** Runs one apply or undo at a time; key repeat and double clicks must not send a batch twice. */
  const whileApplying = async (work: () => Promise<void>) => {
    set({ applying: true });
    try {
      await work();
    } finally {
      set({ applying: false });
    }
  };

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

    async newStageToken() {
      try {
        set({ settings: await api.stageNewToken() });
        get().toast("success", "New token; register DataBee with Stage again");
      } catch (e) {
        fail(e, "Could not replace the token");
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
          wpNav: HOME_NAV,
          mode: "db",
          modeChosen: false,
          selectedCell: null,
        }));
        // Reconnecting the same identity (e.g. a second launch) keeps its query tab.
        const queryTab = get().tabs.find((t) => t.kind === "query" && t.sessionId === session.id);
        if (queryTab) get().activateTab(queryTab.id);
        else get().openQueryTab();
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
        const wasActive = s.activeSessionId === sessionId;
        const sessions = s.sessions.filter((x) => x.id !== sessionId);
        const tabs = s.tabs.filter((t) => t.sessionId !== sessionId);
        const activeSessionId = wasActive ? (sessions[0]?.id ?? null) : s.activeSessionId;
        const activeTabId = tabs.some((t) => t.id === s.activeTabId)
          ? s.activeTabId
          : (tabs.find((t) => t.sessionId === activeSessionId)?.id ?? null);
        const selectionKept = tabs.some((t) => t.id === s.selectedCell?.tabId);
        const { [sessionId]: _schema, ...schema } = s.schema;
        const { [sessionId]: _counter, ...queryCounter } = s.queryCounter;
        const wp = Object.fromEntries(Object.entries(s.wp).filter(([key]) => !key.startsWith(`${sessionId}/`)));
        return {
          sessions,
          tabs,
          activeSessionId,
          activeTabId,
          schema,
          queryCounter,
          wp,
          pending: s.pending.filter((p) => p.sessionId !== sessionId),
          applied: s.applied.filter((a) => a.sessionId !== sessionId),
          selectedCell: selectionKept ? s.selectedCell : null,
          // The entity page and WordPress history describe the active session only.
          ...(wasActive ? { wpNav: HOME_NAV, entityMeta: null } : {}),
        };
      });
      void get().loadFavorites();
    },

    setActiveSession(sessionId) {
      const tab = get().tabs.find((t) => t.sessionId === sessionId);
      set((s) => ({ activeSessionId: sessionId, wpNav: HOME_NAV, ...activate(s, tab?.id ?? null) }));
      void get().loadFavorites();
    },

    async switchDatabase(sessionId, database) {
      try {
        await api.useDatabase(sessionId, database);
        set((s) => ({
          sessions: s.sessions.map((x) => (x.id === sessionId ? { ...x, database } : x)),
          wpNav: HOME_NAV,
          selectedCell: null,
          entityMeta: null,
          // Only a database known to be WordPress keeps the mode; loadWp switches a newly detected one.
          ...(s.wp[`${sessionId}/${database}`] ? {} : { mode: "db" as Mode, modeChosen: false }),
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

    openInDatabase(table, where) {
      const database = selectActiveSession(get())?.database;
      if (!database) return;
      get().setMode("db");
      get().openTableTab(database, table, "data", where);
    },

    openTableTab(database, table, view, filter, opts = {}) {
      const session = selectActiveSession(get());
      if (!session) return;
      remember({ kind: "table", database, table });
      const existing = get().tabs.find(
        (t): t is TableTab =>
          t.kind === "table" && t.sessionId === session.id && t.database === database && t.table === table && t.filter === filter,
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
        title: filter ? `${table} (filtered)` : table,
        database,
        table,
        view: view ?? "data",
        filter,
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

    pinTab(id) {
      set((s) => ({ tabs: s.tabs.map((t) => (t.id === id && t.kind === "table" && t.preview ? { ...t, preview: false } : t)) }));
    },

    setMode(mode) {
      set({ mode, modeChosen: true });
    },

    navigate(route) {
      set((s) => {
        const current = s.wpNav.stack[s.wpNav.index];
        if (current && routeKey(current) === routeKey(route)) return { mode: "wp" as Mode };
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
      const meta = get().entityMeta;
      const title = meta?.key === `${entity}:${id}` && meta.title ? meta.title : `${entity} #${id}`;
      remember({ kind: "entity", entity, id, title });
    },

    bumpData() {
      // Every view reloads, so no selected cell is current anymore.
      set((s) => ({ dataVersion: s.dataVersion + 1, selectedCell: null }));
    },

    async queueChange(change) {
      const session = selectActiveSession(get());
      if (!session) return false;
      try {
        const planned = await api.planChange(session.id, change);
        set((s) => ({ pending: [...s.pending, { ...planned, id: nextPendingId++, sessionId: session.id }] }));
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
      const session = selectActiveSession(get());
      const batch = get().pending.filter((p) => p.sessionId === session?.id);
      if (get().applying || !session || batch.length === 0) return;
      await whileApplying(async () => {
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
      });
    },

    async undoLast() {
      const session = selectActiveSession(get());
      const last = [...get().applied].reverse().find((a) => a.sessionId === session?.id);
      if (get().applying || !session || !last) return;
      const statements = [...last.changes].reverse().flatMap((c) => c.undo);
      if (statements.length === 0) {
        get().toast("info", "This change can't be undone automatically");
        return;
      }
      await whileApplying(async () => {
        try {
          await api.applyChanges(session.id, statements);
          set((s) => ({ applied: s.applied.filter((a) => a !== last) }));
          get().toast("success", "Undone");
          get().bumpData();
        } catch (e) {
          fail(e, "Undo failed");
        }
      });
    },

    async takeSnapshot(entity, id) {
      const session = selectActiveSession(get());
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
        setOverrides(saved);
      } catch (e) {
        fail(e, "Could not save shortcuts");
      }
    },

    setEntityMeta(entityMeta) {
      set({ entityMeta });
      if (entityMeta?.title) retitleRecent(entityMeta.key, entityMeta.title);
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
      set(activate(s, next.id));
    },

    async loadWp(sessionId, database, refresh = false) {
      const key = `${sessionId}/${database}`;
      const token = beginRequest(`wp:${key}`);
      try {
        const info = await api.wpDetect(sessionId, database, refresh);
        if (!isLatest(`wp:${key}`, token)) return;
        set((s) => {
          if (!s.sessions.some((x) => x.id === sessionId)) return {};
          const active = s.activeSessionId === sessionId && selectActiveSession(s)?.database === database;
          return {
            wp: { ...s.wp, [key]: info },
            // WordPress databases open in WordPress mode until the user picks a mode.
            mode: info && active && !s.modeChosen ? "wp" : s.mode,
          };
        });
      } catch (e) {
        if (isLatest(`wp:${key}`, token)) fail(e, "WordPress detection failed");
      }
    },

    updateQueryTab(id, patch) {
      set((s) => ({ tabs: s.tabs.map((t) => (t.id === id && t.kind === "query" ? { ...t, ...patch } : t)) }));
    },

    updateTableTab(id, patch) {
      set((s) => ({ tabs: s.tabs.map((t) => (t.id === id && t.kind === "table" ? { ...t, ...patch } : t)) }));
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
              message: error.message.replace(/^This statement needs confirmation: /, ""),
              confirmLabel: "Run anyway",
              onConfirm: () => void get().runQuery(tabId, sql, true),
            },
          });
        } else {
          get().updateQueryTab(tabId, { running: false, error });
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
      const session = selectActiveSession(get());
      // One key for all sessions: a reply for a session that is no longer active must not land.
      const token = beginRequest("favorites");
      if (!session) {
        set({ favorites: [] });
        return;
      }
      try {
        const favorites = await api.listFavorites(accountOf(session.spec));
        if (isLatest("favorites", token)) set({ favorites });
      } catch (e) {
        if (isLatest("favorites", token)) fail(e, "Could not load favorites");
      }
    },

    async addFavorite(name, target, global = false) {
      const session = selectActiveSession(get());
      if (!session) return;
      try {
        await api.createFavorite({ name, scope: global ? null : accountOf(session.spec), target });
        await get().loadFavorites();
      } catch (e) {
        const error = toAppError(e);
        // Rust refuses a duplicate target; that is a notice, not a failure.
        if (error.kind === "invalid_input") return get().toast("info", error.message);
        fail(e, "Could not add favorite");
      }
    },

    addCurrentTabToFavorites() {
      const tab = get().tabs.find((t) => t.id === get().activeTabId);
      if (!tab) return;
      const saved = (target: FavoriteTarget) => get().favorites.some((f) => JSON.stringify(f.target) === JSON.stringify(target));
      if (tab.kind === "table") {
        const target: FavoriteTarget = { kind: "table", database: tab.database, table: tab.table };
        if (saved(target)) return get().toast("info", `${tab.table} is already in favorites`);
        void get().addFavorite(tab.table, target);
        return;
      }
      if (!tab.sql.trim()) {
        get().toast("info", "Write a query first, then add it to favorites");
        return;
      }
      if (saved({ kind: "query", sql: tab.sql })) return get().toast("info", "This query is already in favorites");
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
        await api.updateFavorite({ ...favorite, name });
        await get().loadFavorites();
      } catch (e) {
        fail(e, "Could not rename favorite");
      }
    },

    async toggleFavoriteScope(favorite) {
      const session = selectActiveSession(get());
      if (!session) return;
      try {
        await api.updateFavorite({ ...favorite, scope: favorite.scope ? null : accountOf(session.spec) });
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
      const previous = get().favorites;
      const ids = previous.map((f) => f.id).filter((x) => x !== id);
      const before = beforeId ? ids.indexOf(beforeId) : -1;
      ids.splice(before >= 0 ? before : ids.length, 0, id);
      const byId = new Map(previous.map((f) => [f.id, f]));
      set({ favorites: ids.flatMap((x) => byId.get(x) ?? []) });
      try {
        await api.reorderFavorites(ids);
      } catch (e) {
        set({ favorites: previous });
        fail(e, "Could not reorder favorites");
      }
    },

    openFavorite(favorite) {
      if (favorite.target.kind === "table") get().openTableTab(favorite.target.database, favorite.target.table);
      else get().openQueryTab(favorite.target.sql, favorite.name);
    },

    toast(kind, message) {
      if (kind === "error") void api.logError(message).catch(() => {});
      const same = get().toasts.find((t) => t.kind === kind && t.message === message);
      if (same) {
        set((s) => ({ toasts: s.toasts.map((t) => (t.id === same.id ? { ...t, count: t.count + 1 } : t)) }));
        return;
      }
      const id = nextToastId++;
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

    selectCell(selected) {
      set((s) => ({ selectedCell: selected && s.activeTabId ? { ...selected, tabId: s.activeTabId } : null }));
    },

    setSidebarPanel(sidebarPanel) {
      set({ sidebarPanel });
    },
  };
});

const uiWrites = new Map<string, { ui: UiState; timer: ReturnType<typeof setTimeout> }>();

/** Writes UI state at most twice a second per connection (drags fire constantly). */
function persistUiState(identity: string, ui: UiState) {
  clearTimeout(uiWrites.get(identity)?.timer);
  uiWrites.set(identity, { ui, timer: setTimeout(() => writeUiState(identity), 500) });
}

function writeUiState(identity: string) {
  const write = uiWrites.get(identity);
  if (!write) return;
  uiWrites.delete(identity);
  void api.setUiState(identity, write.ui).catch((e) => fail(e, "Could not save view state"));
}

/** Writes debounced UI state now; called when the window unloads. */
export function flushUiState() {
  for (const [identity, { timer }] of [...uiWrites]) {
    clearTimeout(timer);
    writeUiState(identity);
  }
}

/** Keeps the last 20 opened entities and tables for the palette's Recent section. */
function remember(item: RecentNav) {
  useStore.setState((s) => {
    const key = JSON.stringify(item.kind === "entity" ? [item.kind, item.entity, item.id] : [item.kind, item.database, item.table]);
    const rest = s.recentNav.filter((r) => JSON.stringify(r.kind === "entity" ? [r.kind, r.entity, r.id] : [r.kind, r.database, r.table]) !== key);
    return { recentNav: [item, ...rest].slice(0, 20) };
  });
}

/** Updates the title of the matching Recent entity once its page has loaded. */
function retitleRecent(key: string, title: string) {
  useStore.setState((s) => ({
    recentNav: s.recentNav.map((r) => (r.kind === "entity" && `${r.entity}:${r.id}` === key ? { ...r, title } : r)),
  }));
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
