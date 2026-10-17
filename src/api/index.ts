import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppliedBatch,
  ChangeRequest,
  CompareRow,
  DiffEntry,
  NewValue,
  PlannedChange,
  WpChangeRequest,
  SnapshotInfo,
  SnapshotScope,
  ActionSummary,
  BrowserSection,
  UserSummary,
  Dashboard,
  ActionHistogram,
  ErrorKind,
  DocField,
  EntityViewData,
  FindGroup,
  HookFailures,
  PeekFact,
  ListQuery,
  OrderSummary,
  OrderType,
  Page,
  PostSummary,
  ProductSummary,
  SearchHit,
  SessionSummary,
  TimelineEvent,
  AppError,
  DecodedValue,
  OptionsOverview,
  WpInfo,
  BrowsePage,
  BrowseRequest,
  OpenArgs,
  DatabaseInfo,
  ExecuteResponse,
  Favorite,
  NewFavorite,
  HistoryEntry,
  Keybindings,
  LaunchEvent,
  RecentConnection,
  Resolved,
  SessionInfo,
  Settings,
  TableInfo,
  Provenance,
  TableReferences,
  PresetInfo,
  RulesInfo,
  Coverage,
  Document,
  ObjectAction,
  ResolvedReference,
  Project,
  ProjectChange,
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
  /** Rust validates the slug (`[a-z0-9-]`, unique among saved connections) and renames open sessions too. */
  renameConnectionSlug: (identity: string, slug: string) => invoke<void>("rename_connection_slug", { identity, slug }),

  connect: (args: OpenArgs) => invoke<SessionInfo>("connect", { args }),
  disconnect: (sessionId: string) => invoke<void>("disconnect", { sessionId }),
  listDatabases: (sessionId: string) => invoke<DatabaseInfo[]>("list_databases", { sessionId }),
  listTables: (sessionId: string, database: string) =>
    invoke<TableInfo[]>("list_tables", { sessionId, database }),
  describeTable: (sessionId: string, database: string, table: string) =>
    invoke<TableStructure>("describe_table", { sessionId, database, table }),
  /** Drops the session's schema cache, so DDL made outside the app shows. */
  refreshSchema: (sessionId: string) => invoke<void>("refresh_schema", { sessionId }),
  completionSchema: (sessionId: string, database: string) =>
    invoke<Record<string, string[]>>("completion_schema", { sessionId, database }),
  useDatabase: (sessionId: string, database: string) =>
    invoke<void>("use_database", { sessionId, database }),
  execute: (sessionId: string, sql: string, confirmed = false) =>
    invoke<ExecuteResponse>("execute", { sessionId, sql, confirmed }),
  cancelQuery: (sessionId: string) => invoke<void>("cancel_query", { sessionId }),
  /** The full text of a `longText` cell of the result set `resultId`. */
  fetchCell: (sessionId: string, resultId: number, row: number, col: number) =>
    invoke<string>("fetch_cell", { sessionId, resultId, row, col }),
  browseTable: (sessionId: string, request: BrowseRequest) =>
    invoke<BrowsePage>("browse_table", { sessionId, request }),
  /** Checks an address against the session (slug, database, table, key) and returns its canonical form. */
  resolveAddress: (sessionId: string, address: string) => invoke<Resolved>("resolve_address", { sessionId, address }),
  /** Declared foreign keys, project and preset references, and same-name references from and to a table. */
  tableReferences: (sessionId: string, database: string, table: string) =>
    invoke<TableReferences>("table_references", { sessionId, database, table }),
  /** The value at `provenance` as a SQL condition (`meta_key = '…' AND meta_value = '…'` on a meta value); `raw: null` is NULL. */
  valueCondition: (sessionId: string, database: string, provenance: Provenance, raw: string | null) =>
    invoke<string>("value_condition", { sessionId, database, provenance, raw }),
  presetsList: () => invoke<PresetInfo[]>("presets_list"),
  /** The prefixes, applied presets and entities of a database. */
  rulesInfo: (sessionId: string, database: string) => invoke<RulesInfo>("rules_info", { sessionId, database }),
  /** The project of the session's connection and `database`; empty when none is stored. */
  projectGet: (sessionId: string, database: string) => invoke<Project>("project_get", { sessionId, database }),
  /** Checks the change against the database's tables and columns; errors name the field. */
  projectUpdate: (sessionId: string, database: string, change: ProjectChange) =>
    invoke<Project>("project_update", { sessionId, database, change }),
  /** The project's rules as a preset file (TOML). */
  projectExport: (sessionId: string, database: string) => invoke<string>("project_export", { sessionId, database }),
  /** How many source rows of a reference (as `tableReferences` returned it) find a target row; read-only. */
  referenceCoverage: (sessionId: string, database: string, reference: ResolvedReference) =>
    invoke<Coverage>("reference_coverage", { sessionId, database, reference }),
  /** The document of a row address, or of an entity shorthand (`order 4711`) in `database`. */
  document: (sessionId: string, address: string, database: string | null) =>
    invoke<Document>("document", { sessionId, address, database }),
  /** The `scope = "table"` rule actions of `table`. */
  tableActions: (sessionId: string, database: string, table: string) =>
    invoke<ObjectAction[]>("table_actions", { sessionId, database, table }),
  /** Plans a rule action on a row address, or on a table address without a filter (table actions). */
  planAction: (sessionId: string, address: string, action: string, params: Record<string, string>) =>
    invoke<PlannedChange>("plan_action", { sessionId, address, action, params }),
  /** The "New query" SQL for a table, quoted by Rust. */
  tableSelectSql: (sessionId: string, database: string, table: string) =>
    invoke<string>("table_select_sql", { sessionId, database, table }),

  listFavorites: (account: string) => invoke<Favorite[]>("list_favorites", { account }),
  createFavorite: (favorite: NewFavorite) => invoke<Favorite>("create_favorite", { favorite }),
  updateFavorite: (favorite: Favorite) => invoke<Favorite>("update_favorite", { favorite }),
  removeFavorite: (id: string) => invoke<void>("remove_favorite", { id }),
  reorderFavorites: (ids: string[]) => invoke<void>("reorder_favorites", { ids }),

  listHistory: (account: string) => invoke<HistoryEntry[]>("list_history", { account }),
  clearHistory: (account: string) => invoke<void>("clear_history", { account }),

  wpDetect: (sessionId: string, database: string, refresh = false) =>
    invoke<WpInfo | null>("plugin:wp|wp_detect", { sessionId, database, refresh }),
  wpOptions: (sessionId: string, database: string) => invoke<OptionsOverview>("plugin:wp|wp_options", { sessionId, database }),
  decodeValue: (text: string) => invoke<DecodedValue | null>("decode_value", { text }),
  wpDashboard: (sessionId: string, database: string) => invoke<Dashboard>("plugin:wp|wp_dashboard", { sessionId, database }),
  wpBrowser: (sessionId: string, database: string) => invoke<BrowserSection[]>("plugin:wp|wp_browser", { sessionId, database }),
  wpOrders: (sessionId: string, database: string, orderType: OrderType, query: ListQuery) =>
    invoke<Page<OrderSummary>>("plugin:wp|wp_orders", { sessionId, database, orderType, query }),
  wpUsers: (sessionId: string, database: string, query: ListQuery) =>
    invoke<Page<UserSummary>>("plugin:wp|wp_users", { sessionId, database, query }),
  wpProducts: (sessionId: string, database: string, query: ListQuery) =>
    invoke<Page<ProductSummary>>("plugin:wp|wp_products", { sessionId, database, query }),
  wpActions: (sessionId: string, database: string, query: ListQuery) =>
    invoke<Page<ActionSummary>>("plugin:wp|wp_actions", { sessionId, database, query }),
  wpFailedByHook: (sessionId: string, database: string) => invoke<HookFailures[]>("plugin:wp|wp_failed_by_hook", { sessionId, database }),
  wpActionHistogram: (sessionId: string, database: string, hook: string | null) =>
    invoke<ActionHistogram>("plugin:wp|wp_action_histogram", { sessionId, database, hook }),
  wpSessions: (sessionId: string, database: string, query: ListQuery) =>
    invoke<Page<SessionSummary>>("plugin:wp|wp_sessions", { sessionId, database, query }),
  wpContent: (sessionId: string, database: string, query: ListQuery) =>
    invoke<Page<PostSummary>>("plugin:wp|wp_content", { sessionId, database, query }),
  wpView: (sessionId: string, database: string, entity: string, id: string) =>
    invoke<EntityViewData>("plugin:wp|wp_view", { sessionId, database, entity, id }),
  wpOrderLogs: (sessionId: string, database: string, id: string) =>
    invoke<TimelineEvent[]>("plugin:wp|wp_order_logs", { sessionId, database, id }),
  wpPeek: (sessionId: string, database: string, entity: string, id: string) =>
    invoke<PeekFact[]>("plugin:wp|wp_peek", { sessionId, database, entity, id }),
  wpFindValue: (sessionId: string, database: string, value: string) =>
    invoke<FindGroup[]>("plugin:wp|wp_find_value", { sessionId, database, value }),
  wpSearch: (sessionId: string, database: string, query: string) =>
    invoke<SearchHit[]>("plugin:wp|wp_search", { sessionId, database, query }),
  wpOptionsGroup: (sessionId: string, database: string, prefix: string) =>
    invoke<DocField[]>("plugin:wp|wp_options_group", { sessionId, database, prefix }),

  planChange: (sessionId: string, change: ChangeRequest) => invoke<PlannedChange>("plan_change", { sessionId, change }),
  wpPlanChange: (sessionId: string, change: WpChangeRequest) => invoke<PlannedChange>("plugin:wp|wp_plan_change", { sessionId, change }),
  discardChange: (sessionId: string, id: number) => invoke<void>("discard_change", { sessionId, id }),
  applyChanges: (sessionId: string, ids: number[]) => invoke<AppliedBatch>("apply_changes", { sessionId, ids }),
  undoBatch: (sessionId: string, batchId: number) => invoke<number>("undo_batch", { sessionId, batchId }),
  editSerialized: (raw: string, path: number[], value: NewValue) => invoke<string>("edit_serialized", { raw, path, value }),
  wpSnapshotTake: (sessionId: string, database: string, scope: SnapshotScope, label: string) =>
    invoke<SnapshotInfo>("plugin:wp|wp_snapshot_take", { sessionId, database, scope, label }),
  wpSnapshots: (sessionId: string, database: string) => invoke<SnapshotInfo[]>("plugin:wp|wp_snapshots", { sessionId, database }),
  wpSnapshotDiff: (sessionId: string, database: string, id: string) => invoke<DiffEntry[]>("plugin:wp|wp_snapshot_diff", { sessionId, database, id }),
  wpSnapshotDelete: (id: string) => invoke<void>("plugin:wp|wp_snapshot_delete", { id }),
  wpCompare: (sessionId: string, database: string, entity: string, a: string, b: string) =>
    invoke<CompareRow[]>("plugin:wp|wp_compare", { sessionId, database, entity, a, b }),

  onLaunch: (handler: () => void): Promise<UnlistenFn> => listen("launch", handler),
  /** Native menu items emit the id of the command they run. */
  onCommand: (handler: (id: string) => void): Promise<UnlistenFn> =>
    listen<string>("command", (e) => handler(e.payload)),
};

/** Every `ErrorKind` Rust sends; the `Record` makes the compiler check the list is complete. */
const ERROR_KINDS: Record<ErrorKind, true> = {
  database: true,
  connection: true,
  auth_failed: true,
  password_required: true,
  timeout: true,
  cancelled: true,
  confirmation_required: true,
  session_not_found: true,
  invalid_input: true,
  keychain: true,
  io: true,
};

export function toAppError(e: unknown): AppError {
  if (e && typeof e === "object" && "kind" in e && "message" in e && Object.hasOwn(ERROR_KINDS, String(e.kind))) {
    return e as AppError;
  }
  const message = e && typeof e === "object" && "message" in e ? String(e.message) : String(e);
  return { kind: "io", message, code: null, account: null };
}
