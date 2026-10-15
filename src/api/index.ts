import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ChangeRequest,
  CompareRow,
  DiffEntry,
  NewValue,
  PlannedChange,
  SnapshotInfo,
  SnapshotScope,
  ActionSummary,
  BrowserSection,
  CustomerSummary,
  Dashboard,
  DayBucket,
  DocField,
  EntityViewData,
  FindGroup,
  HookFailures,
  LensReport,
  PeekFact,
  ListQuery,
  OrderSummary,
  Page,
  PostSummary,
  ProductSummary,
  SearchHit,
  SessionSummary,
  TimelineEvent,
  AppError,
  DecodedValue,
  EntityDoc,
  OptionsOverview,
  WpInfo,
  BrowsePage,
  BrowseRequest,
  ConnectionSpec,
  DatabaseInfo,
  ExecuteResponse,
  Favorite,
  HistoryEntry,
  Keybindings,
  LaunchEvent,
  RecentConnection,
  SessionInfo,
  Settings,
  TableInfo,
  TableStructure,
  UiState,
} from "./types";

export * from "./types";

/** The only place that talks to Rust. Tests mock this module. */
export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  updateSettings: (settings: Settings) => invoke<Settings>("update_settings", { settings }),
  getKeybindings: () => invoke<Keybindings>("get_keybindings"),
  setKeybindings: (bindings: Keybindings) => invoke<Keybindings>("set_keybindings", { bindings }),
  getUiState: (identity: string) => invoke<UiState>("get_ui_state", { identity }),
  setUiState: (identity: string, ui: UiState) => invoke<UiState>("set_ui_state", { identity, ui }),
  openLogFolder: () => invoke<void>("open_log_folder"),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
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

  wpDetect: (sessionId: string, database: string, refresh = false) =>
    invoke<WpInfo | null>("wp_detect", { sessionId, database, refresh }),
  wpLoad: (sessionId: string, database: string, entity: string, ids: string[]) =>
    invoke<EntityDoc[]>("wp_load", { sessionId, database, entity, ids }),
  wpExpand: (sessionId: string, database: string, entity: string, id: string, relation: string) =>
    invoke<EntityDoc[]>("wp_expand", { sessionId, database, entity, id, relation }),
  wpOptions: (sessionId: string, database: string) => invoke<OptionsOverview>("wp_options", { sessionId, database }),
  wpTransientCleanupSql: (sessionId: string, database: string) =>
    invoke<string>("wp_transient_cleanup_sql", { sessionId, database }),
  decodeValue: (text: string) => invoke<DecodedValue | null>("decode_value", { text }),
  wpDashboard: (sessionId: string, database: string) => invoke<Dashboard>("wp_dashboard", { sessionId, database }),
  wpBrowser: (sessionId: string, database: string) => invoke<BrowserSection[]>("wp_browser", { sessionId, database }),
  wpOrders: (sessionId: string, database: string, orderType: string, query: ListQuery) =>
    invoke<Page<OrderSummary>>("wp_orders", { sessionId, database, orderType, query }),
  wpCustomers: (sessionId: string, database: string, query: ListQuery) =>
    invoke<Page<CustomerSummary>>("wp_customers", { sessionId, database, query }),
  wpProducts: (sessionId: string, database: string, query: ListQuery) =>
    invoke<Page<ProductSummary>>("wp_products", { sessionId, database, query }),
  wpActions: (sessionId: string, database: string, query: ListQuery) =>
    invoke<Page<ActionSummary>>("wp_actions", { sessionId, database, query }),
  wpFailedByHook: (sessionId: string, database: string) => invoke<HookFailures[]>("wp_failed_by_hook", { sessionId, database }),
  wpActionHistogram: (sessionId: string, database: string, hook: string | null) =>
    invoke<DayBucket[]>("wp_action_histogram", { sessionId, database, hook }),
  wpSessions: (sessionId: string, database: string, query: ListQuery) =>
    invoke<Page<SessionSummary>>("wp_sessions", { sessionId, database, query }),
  wpContent: (sessionId: string, database: string, query: ListQuery) =>
    invoke<Page<PostSummary>>("wp_content", { sessionId, database, query }),
  wpView: (sessionId: string, database: string, entity: string, id: string) =>
    invoke<EntityViewData>("wp_view", { sessionId, database, entity, id }),
  wpOrderLogs: (sessionId: string, database: string, id: string) =>
    invoke<TimelineEvent[]>("wp_order_logs", { sessionId, database, id }),
  wpLens: (sessionId: string, database: string) => invoke<LensReport>("wp_lens", { sessionId, database }),
  wpPluginAssign: (key: string, slug: string | null) => invoke<Record<string, string>>("wp_plugin_assign", { key, slug }),
  wpPeek: (sessionId: string, database: string, entity: string, id: string) =>
    invoke<PeekFact[]>("wp_peek", { sessionId, database, entity, id }),
  wpFindValue: (sessionId: string, database: string, value: string) =>
    invoke<FindGroup[]>("wp_find_value", { sessionId, database, value }),
  wpSearch: (sessionId: string, database: string, query: string) =>
    invoke<SearchHit[]>("wp_search", { sessionId, database, query }),
  wpOptionsGroup: (sessionId: string, database: string, prefix: string) =>
    invoke<DocField[]>("wp_options_group", { sessionId, database, prefix }),

  planChange: (sessionId: string, change: ChangeRequest) => invoke<PlannedChange>("plan_change", { sessionId, change }),
  applyChanges: (sessionId: string, statements: string[]) => invoke<number>("apply_changes", { sessionId, statements }),
  editSerialized: (raw: string, path: string[], value: NewValue) => invoke<string>("edit_serialized", { raw, path, value }),
  wpSnapshotTake: (sessionId: string, database: string, scope: SnapshotScope, label: string) =>
    invoke<SnapshotInfo>("wp_snapshot_take", { sessionId, database, scope, label }),
  wpSnapshots: (sessionId: string, database: string) => invoke<SnapshotInfo[]>("wp_snapshots", { sessionId, database }),
  wpSnapshotDiff: (sessionId: string, database: string, id: string) => invoke<DiffEntry[]>("wp_snapshot_diff", { sessionId, database, id }),
  wpSnapshotDelete: (id: string) => invoke<void>("wp_snapshot_delete", { id }),
  wpCompare: (sessionId: string, database: string, entity: string, a: string, b: string) =>
    invoke<CompareRow[]>("wp_compare", { sessionId, database, entity, a, b }),

  onLaunch: (handler: () => void): Promise<UnlistenFn> => listen("launch", handler),
  /** Native menu items emit the id of the command they run. */
  onCommand: (handler: (id: string) => void): Promise<UnlistenFn> =>
    listen<string>("command", (e) => handler(e.payload)),
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
