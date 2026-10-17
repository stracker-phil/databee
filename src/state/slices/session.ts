import { api, toAppError, type AppError, type ConnectionSpec, type DatabaseInfo, type LaunchEvent, type SessionInfo, type TableInfo, type TableStructure } from "../../api";
import { openFromOutside } from "../outside";
import { fail } from "../requests";
import { structureKey } from "../selectors";
import { sessionClosedPatch, type Slice, type Store } from "../store";
import { activate, resetSelection } from "./tabs";

/** A table's `describeTable` reply for one `dataVersion`; the previous structure stays while reloading. */
export interface StructureEntry {
  version: number;
  structure: TableStructure | null;
  error: AppError | null;
}

export interface SchemaState {
  databases: DatabaseInfo[];
  tables: Record<string, TableInfo[]>;
}

export interface SessionSlice {
  sessions: SessionInfo[];
  activeSessionId: string | null;
  schema: Record<string, SchemaState>;
  /** Table structures by `session/database/table`, shared by every tab of that table. */
  structures: Record<string, StructureEntry>;
  handleLaunchEvents: (events: LaunchEvent[]) => Promise<void>;
  connect: (args: { spec: ConnectionSpec; password?: string; requestId?: string; savePassword?: boolean }) => Promise<boolean>;
  disconnect: (sessionId: string) => Promise<void>;
  setActiveSession: (sessionId: string) => void;
  switchDatabase: (sessionId: string, database: string) => Promise<void>;
  loadDatabases: (sessionId: string) => Promise<void>;
  loadTables: (sessionId: string, database: string) => Promise<void>;
  /** Describes the table once per `dataVersion`; the reply lands in `structures`. */
  loadStructure: (sessionId: string, database: string, table: string) => Promise<void>;
}

export function nextActiveSessionId(s: Store, closed: string) {
  if (s.activeSessionId !== closed) return s.activeSessionId;
  return s.sessions.find((x) => x.id !== closed)?.id ?? null;
}

/** Drops every `<session>/…` key of a record keyed by session. */
export function withoutSession<T>(record: Record<string, T>, sessionId: string): Record<string, T> {
  return Object.fromEntries(Object.entries(record).filter(([key]) => !key.startsWith(`${sessionId}/`)));
}

export const sessionSlice: Slice<SessionSlice> = {
  create: (set, get) => ({
    sessions: [],
    activeSessionId: null,
    schema: {},
    structures: {},

    async handleLaunchEvents(events) {
      // An open after a failed connect would only add a toast to the connect or password dialog.
      let connected = true;
      for (const event of events) {
        if (event.type === "error") {
          get().toast("error", event.message);
        } else if (event.type === "open") {
          if (!connected) continue;
          await openFromOutside(get, event.address, event.connection).catch((e) => get().toast("error", toAppError(e).message));
        } else {
          connected = await get().connect({
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
          ...resetSelection(),
        }));
        // Reconnecting the same identity (e.g. a second launch) keeps its query tab.
        const queryTab = get().tabs.find((t) => t.kind === "query" && t.sessionId === session.id);
        if (queryTab) get().activateTab(queryTab.id);
        else get().openQueryTab();
        get().openReadout({ sessionId: session.id });
        void get().loadDatabases(session.id);
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
              account: error.account ?? null,
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
      // One update, so no render shows a closed session's leftovers.
      set((s) => sessionClosedPatch(sessionId, s));
    },

    setActiveSession(sessionId) {
      const tab = get().tabs.find((t) => t.sessionId === sessionId);
      set((s) => ({ activeSessionId: sessionId, ...activate(s, tab?.id ?? null) }));
    },

    async switchDatabase(sessionId, database) {
      try {
        await api.useDatabase(sessionId, database);
        set((s) => ({
          sessions: s.sessions.map((x) => (x.id === sessionId ? { ...x, database } : x)),
          ...resetSelection(),
        }));
        get().openReadout({ sessionId, database, activate: false });
        void get().loadTables(sessionId, database);
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
        if (current) void get().loadTables(sessionId, current);
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

    async loadStructure(sessionId, database, table) {
      const key = structureKey(sessionId, database, table);
      const version = get().dataVersion;
      const cached: StructureEntry | undefined = get().structures[key];
      if (cached?.version === version) return;
      const write = (entry: Omit<StructureEntry, "version">) => set((s) => ({ structures: { ...s.structures, [key]: { ...entry, version } } }));
      // A newer version or a disconnect (which drops the entry) makes this reply stale.
      const current = () => get().structures[key]?.version === version;
      // Claiming the version up front dedupes concurrent callers; the old structure stays visible meanwhile.
      write({ structure: cached?.structure ?? null, error: cached?.error ?? null });
      try {
        const structure = await api.describeTable(sessionId, database, table);
        if (current()) write({ structure, error: null });
      } catch (e) {
        if (current()) write({ structure: null, error: toAppError(e) });
      }
    },
  }),

  sessionClosed(sessionId, s) {
    const { [sessionId]: _schema, ...schema } = s.schema;
    return {
      sessions: s.sessions.filter((x) => x.id !== sessionId),
      activeSessionId: nextActiveSessionId(s, sessionId),
      schema,
      structures: withoutSession(s.structures, sessionId),
    };
  },
};
