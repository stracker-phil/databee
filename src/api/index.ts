import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppliedBatch,
  ChangeRequest,
  ColumnEquals,
  NewValue,
  PlannedChange,
  ErrorKind,
  AppError,
  DecodedValue,
  Peek,
  Readout,
  PaletteHit,
  BrowsePage,
  BrowseRequest,
  OpenArgs,
  DatabaseInfo,
  ExecuteResponse,
  HistoryEntry,
  Keybindings,
  LaunchEvent,
  RemoteEvent,
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
  Comparison,
  SnapshotDiff,
  SnapshotInfo,
  WatchPoll,
  ObjectAction,
  ActionPreview,
  ResolvedReference,
  Project,
  ProjectChange,
  ProjectRules,
  TableProfile,
  Reference,
  RuleDraft,
  SavedEntry,
  SavedQuery,
  Home,
  ValueCount,
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
  /** The native save panel; the chosen path, or null when cancelled. */
  chooseSavePath: (fileName: string, extension: string) => invoke<string | null>("choose_save_path", { fileName, extension }),
  logError: (message: string) => invoke<void>("log_frontend_error", { message }),

  takeLaunchEvents: () => invoke<LaunchEvent[]>("take_launch_events"),
  /** Answers a `remote` event; a non-null `error` fails the request. */
  remoteReply: (id: string, result: unknown, error: string | null) => invoke<void>("remote_reply", { id, result, error }),
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
  /** Writes the project's rules as a preset file to `path`. */
  projectExportTo: (sessionId: string, database: string, path: string) => invoke<void>("project_export_to", { sessionId, database, path }),
  /** Every reference (switched off ones too), kind, value style and preset of a database, for the Project page. */
  projectRules: (sessionId: string, database: string) => invoke<ProjectRules>("project_rules", { sessionId, database }),
  /** A table's Structure view: per column its kinds, references and value styles, then its meta-key rules, incoming references, entities and actions. */
  tableRules: (sessionId: string, database: string, table: string) => invoke<TableProfile>("table_rules", { sessionId, database, table }),
  /** The start of a rule form for a value: its column as rules name it and, on a meta value, its meta key. */
  ruleDraft: (sessionId: string, database: string, provenance: Provenance) => invoke<RuleDraft>("rule_draft", { sessionId, database, provenance }),
  /** The coverage of an unsaved reference rule; `replacing` is the project reference it would replace. Errors name the field. */
  coveragePreview: (sessionId: string, database: string, reference: Reference, replacing: string | null) =>
    invoke<Coverage>("coverage_preview", { sessionId, database, reference, replacing }),
  /** How many source rows of a reference (as `tableReferences` returned it) find a target row; read-only. */
  referenceCoverage: (sessionId: string, database: string, reference: ResolvedReference) =>
    invoke<Coverage>("reference_coverage", { sessionId, database, reference }),
  /** The document of a row address, or of an entity shorthand (`order 4711`) in `database`. */
  document: (sessionId: string, address: string, database: string | null) =>
    invoke<Document>("document", { sessionId, address, database }),
  /** What a peek shows of a row address: its entity's summary, else its first columns. */
  peek: (sessionId: string, address: string) => invoke<Peek>("peek", { sessionId, address }),
  /** Starts live mode on a row address; returns the watch id (at most 8 per session, the oldest ends). */
  watchStart: (sessionId: string, address: string) => invoke<string>("watch_start", { sessionId, address }),
  /** Reads the watched row again: what changed since the last poll, and the new document when anything did. */
  watchPoll: (sessionId: string, id: string) => invoke<WatchPoll>("watch_poll", { sessionId, id }),
  watchStop: (sessionId: string, id: string) => invoke<void>("watch_stop", { sessionId, id }),
  /** Both documents of a row address with `?compare=<key>` and their differences. */
  compare: (sessionId: string, address: string) => invoke<Comparison>("compare", { sessionId, address }),
  /** Stores the row or saved query (`…/q/<id>`) at `address` as it is now. */
  snapshotTake: (sessionId: string, address: string, label: string) => invoke<SnapshotInfo>("snapshot_take", { sessionId, address, label }),
  /** Snapshots of `database`, newest first; only those of `address` when given. */
  snapshotList: (sessionId: string, database: string, address: string | null) =>
    invoke<SnapshotInfo[]>("snapshot_list", { sessionId, database, address }),
  /** What changed since a snapshot, read now. */
  snapshotDiff: (sessionId: string, id: string) => invoke<SnapshotDiff>("snapshot_diff", { sessionId, id }),
  snapshotDelete: (id: string) => invoke<void>("snapshot_delete", { id }),
  /** The palette's row hits for `#id`, `@name`, `=value` or free text of 6+ characters. */
  paletteSearch: (sessionId: string, database: string, query: string) =>
    invoke<PaletteHit[]>("palette_search", { sessionId, database, query }),
  /** The `scope = "table"` rule actions of `table`. */
  tableActions: (sessionId: string, database: string, table: string) =>
    invoke<ObjectAction[]>("table_actions", { sessionId, database, table }),
  /** Plans a rule action on a row address, or on a table address without a filter (table actions), without
   * queueing it: the rows and values it writes. `queueActionPreview` queues exactly that plan. */
  previewAction: (sessionId: string, address: string, action: string, params: Record<string, string>) =>
    invoke<ActionPreview>("preview_action", { sessionId, address, action, params }),
  queueActionPreview: (sessionId: string, id: number) => invoke<PlannedChange>("queue_action_preview", { sessionId, id }),
  /** The "New query" SQL for a table, quoted by Rust. */
  tableSelectSql: (sessionId: string, database: string, table: string) =>
    invoke<string>("table_select_sql", { sessionId, database, table }),

  /** The project's saved queries, the global ones and the applied presets' pinned queries, `{p}` replaced. */
  savedQueries: (sessionId: string, database: string) => invoke<SavedEntry[]>("saved_queries", { sessionId, database }),
  /** Stores a new query in the project; Rust refuses an empty title. */
  saveQuery: (sessionId: string, database: string, title: string, sql: string, pin: Home | null) =>
    invoke<SavedQuery>("save_query", { sessionId, database, title, sql, pin }),
  /** Replaces the project or global query with the same id; preset queries are read-only. */
  updateSavedQuery: (sessionId: string, database: string, query: SavedQuery) =>
    invoke<SavedQuery>("update_saved_query", { sessionId, database, query }),
  removeSavedQuery: (sessionId: string, database: string, id: string) => invoke<void>("remove_saved_query", { sessionId, database, id }),
  /** Copies a preset's pinned query into the project, where it can be changed. */
  copyPresetQueryToProject: (sessionId: string, database: string, preset: string, id: string) =>
    invoke<SavedQuery>("copy_preset_query_to_project", { sessionId, database, preset, id }),
  /** The most frequent values of `table.column` among the rows `equals` selects, with their counts. */
  valueCounts: (sessionId: string, database: string, table: string, column: string, equals: ColumnEquals[], limit: number) =>
    invoke<ValueCount[]>("value_counts", { sessionId, database, table, column, equals, limit }),
  /** Why the SQL cannot take one more condition (only a single plain single-table SELECT can), or null. */
  narrowRefusal: (sql: string) => invoke<string | null>("narrow_refusal", { sql }),
  /** The SQL with `column = value` (IS NULL for null) added to its WHERE. */
  narrowQuery: (sessionId: string, sql: string, column: string, value: string | null) =>
    invoke<string>("narrow_query", { sessionId, sql, column, value }),

  listHistory: (account: string) => invoke<HistoryEntry[]>("list_history", { account }),
  clearHistory: (account: string) => invoke<void>("clear_history", { account }),

  /** The home page of `database`; a part that fails carries its error instead of failing the readout. */
  readout: (sessionId: string, database: string) => invoke<Readout>("readout", { sessionId, database }),
  /** Records a row address as the newest recent object of its project. */
  rememberObject: (sessionId: string, address: string) => invoke<void>("remember_object", { sessionId, address }),
  decodeValue: (text: string) => invoke<DecodedValue | null>("decode_value", { text }),

  planChange: (sessionId: string, change: ChangeRequest) => invoke<PlannedChange>("plan_change", { sessionId, change }),
  discardChange: (sessionId: string, id: number) => invoke<void>("discard_change", { sessionId, id }),
  applyChanges: (sessionId: string, ids: number[]) => invoke<AppliedBatch>("apply_changes", { sessionId, ids }),
  undoBatch: (sessionId: string, batchId: number) => invoke<number>("undo_batch", { sessionId, batchId }),
  editSerialized: (raw: string, path: number[], value: NewValue) => invoke<string>("edit_serialized", { raw, path, value }),

  onLaunch: (handler: () => void): Promise<UnlistenFn> => listen("launch", handler),
  /** Native menu items emit the id of the command they run. */
  onCommand: (handler: (id: string) => void): Promise<UnlistenFn> =>
    listen<string>("command", (e) => handler(e.payload)),
  /** Requests from other processes (`databee open`, `databee command`); answer each with `remoteReply`. */
  onRemote: (handler: (event: RemoteEvent) => void): Promise<UnlistenFn> =>
    listen<RemoteEvent>("remote", (e) => handler(e.payload)),
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
