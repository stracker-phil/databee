import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppError,
  BrowsePage,
  BrowseRequest,
  ConnectionSpec,
  DatabaseInfo,
  ExecuteResponse,
  Favorite,
  HistoryEntry,
  LaunchEvent,
  RecentConnection,
  SessionInfo,
  Settings,
  TableInfo,
  TableStructure,
} from "./types";

export * from "./types";

/** The only place that talks to Rust. Tests mock this module. */
export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  updateSettings: (settings: Settings) => invoke<Settings>("update_settings", { settings }),
  openLogFolder: () => invoke<void>("open_log_folder"),
  logError: (message: string) => invoke<void>("log_frontend_error", { message }),

  takeLaunchEvents: () => invoke<LaunchEvent[]>("take_launch_events"),
  listRecents: () => invoke<RecentConnection[]>("list_recents"),
  removeRecent: (identity: string) => invoke<void>("remove_recent", { identity }),
  forgetPassword: (spec: ConnectionSpec) => invoke<void>("forget_password", { spec }),

  connect: (args: {
    spec: ConnectionSpec;
    password?: string;
    requestId?: string;
    savePassword?: boolean;
  }) => invoke<SessionInfo>("connect", { args }),
  disconnect: (sessionId: string) => invoke<void>("disconnect", { sessionId }),
  listDatabases: (sessionId: string) => invoke<DatabaseInfo[]>("list_databases", { sessionId }),
  listTables: (sessionId: string, database: string) =>
    invoke<TableInfo[]>("list_tables", { sessionId, database }),
  describeTable: (sessionId: string, database: string, table: string) =>
    invoke<TableStructure>("describe_table", { sessionId, database, table }),
  completionSchema: (sessionId: string, database: string) =>
    invoke<Record<string, string[]>>("completion_schema", { sessionId, database }),
  useDatabase: (sessionId: string, database: string) =>
    invoke<void>("use_database", { sessionId, database }),
  execute: (sessionId: string, sql: string, confirmed = false) =>
    invoke<ExecuteResponse>("execute", { sessionId, sql, confirmed }),
  cancelQuery: (sessionId: string) => invoke<void>("cancel_query", { sessionId }),
  browseTable: (sessionId: string, request: BrowseRequest) =>
    invoke<BrowsePage>("browse_table", { sessionId, request }),

  listFavorites: (account: string) => invoke<Favorite[]>("list_favorites", { account }),
  saveFavorite: (favorite: Favorite) => invoke<Favorite>("save_favorite", { favorite }),
  removeFavorite: (id: string) => invoke<void>("remove_favorite", { id }),
  reorderFavorites: (ids: string[]) => invoke<void>("reorder_favorites", { ids }),

  listHistory: (account: string) => invoke<HistoryEntry[]>("list_history", { account }),
  clearHistory: (account: string) => invoke<void>("clear_history", { account }),

  onLaunch: (handler: () => void): Promise<UnlistenFn> => listen("launch", handler),
  onMenu: (handler: (id: string) => void): Promise<UnlistenFn> =>
    listen<string>("menu", (e) => handler(e.payload)),
};

export function toAppError(e: unknown): AppError {
  if (e && typeof e === "object" && "kind" in e && "message" in e) {
    return e as AppError;
  }
  return { kind: "io", message: e instanceof Error ? e.message : String(e), code: null };
}

/** `user@host:port` — matches `ConnectionSpec::account` in Rust. */
export function accountOf(spec: ConnectionSpec): string {
  return spec.socket ? `${spec.user}@${spec.socket}` : `${spec.user}@${spec.host}:${spec.port}`;
}
