// Mirrors the serialized Rust types in src-tauri/src.

export type Theme = "system" | "light" | "dark";
export type Density = "comfortable" | "compact";
export type LogLevel = "error" | "warn" | "info" | "debug" | "trace";

export interface Settings {
  theme: Theme;
  accent: string;
  /** Chrome size in px; also the rem base. */
  uiFontSize: number;
  /** Grid cells, document values, trees and the SQL editor, in px. */
  dataFontSize: number;
  density: Density;
  rowLimit: number;
  queryTimeoutSecs: number;
  confirmDestructive: boolean;
  nullLabel: string;
  maxCellChars: number;
  connectTimeoutSecs: number;
  defaultPort: number;
  showSystemSchemas: boolean;
  logLevel: LogLevel;
  gravatar: boolean;
  /** Value inspector and WordPress detail pane: beside or below the content. */
  inspectorDock: "right" | "bottom";
  /** The local HTTP API through which Stage presents DataBee. */
  stage: StageSettings;
}

export interface StageSettings {
  enabled: boolean;
  port: number;
  token: string;
}

export interface StageStatus {
  enabled: boolean;
  running: boolean;
  error: string | null;
  url: string;
  /** `stage register …` with this hook's URL and token, ready to paste into a terminal. */
  registerCommand: string;
  /** When a presenter last used the hook (ISO 8601). */
  lastRequest: string | null;
}

/** CSS pixels from the page's top-left. */
export interface StageRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** The windows the Stage hook exposes; settings and the palette are sheets in the main one. */
export type StageWindow = "main" | "settings" | "palette";

/** An element marked `data-stage="<id>"`, as the page measures it. */
export interface PageAnchor {
  id: string;
  label: string;
  window: StageWindow;
  rect: StageRect;
  /** At least half of it shows (or, larger than its scroll area, it fills half of that). */
  inView: boolean;
  /** The part its scroll areas and the window let through; null when nothing shows. */
  visible: StageRect | null;
  role?: string;
  list?: string;
  selected?: boolean;
  checked?: boolean;
  expanded?: boolean;
  disabled?: boolean;
}

/** A key combination as the page dispatches it (`KeyboardEvent` names). */
export interface StageKeys {
  key: string;
  code: string;
  meta: boolean;
  shift: boolean;
  alt: boolean;
  ctrl: boolean;
}

/** Rust → page (`stage:act`). `window` is Rust's own: open or close a sheet. */
export interface StageActRequest {
  request: string;
  kind: "click" | "type" | "reveal" | "scroll" | "keys" | "command" | "window";
  anchor?: string;
  text?: string;
  mode?: "replace" | "append";
  by?: number;
  to?: "top" | "bottom";
  keys?: StageKeys;
  /** Rust focused the window for these keys. */
  focused?: boolean;
  command?: string;
  window?: StageWindow;
  action?: "show" | "hide";
  section?: string;
}

/** Page → Rust (`stage_done`); `handled` is false when the anchor isn't on the page. */
export interface StageActReply {
  request: string;
  page: string;
  handled: boolean;
  ok: boolean;
  error?: string;
  /** For failures that aren't 500, e.g. 409 (not scrollable). */
  status?: number;
  window?: StageWindow;
  rect?: StageRect;
  visible?: StageRect | null;
  /** The command a key press ran (its shortcut). */
  command?: string;
}

export interface StageCommand {
  id: string;
  help: string;
  shortcut?: string;
  enabled: boolean;
  /** Families: the argument after the id prefix, and current values. */
  arg?: string;
  choices?: { id: string; title: string }[];
}

/** A sheet the page draws inside the main window; `rect` null while closed. */
export interface StageSurface {
  id: Exclude<StageWindow, "main">;
  title: string;
  rect: StageRect | null;
  sections?: { id: string; title: string }[];
  section?: string | null;
}

/** Page → Rust (`stage_info`): what the page shows and can do, for Stage's narration. */
export interface StageInfoReply {
  request: string;
  page: string;
  state: Record<string, string | number | boolean | null>;
  /** What the main window shows. */
  view: string | null;
  /** The main window's toggleable panels. */
  ui: Record<string, "open" | "closed">;
  surfaces: StageSurface[];
  commands: StageCommand[];
}

export const ACCENTS = ["blue", "purple", "pink", "graphite"] as const;

/** Shortcut overrides by command id; `null` unbinds. */
export type Keybindings = Record<string, string | null>;

export interface ConnectionSpec {
  name?: string | null;
  host: string;
  port: number;
  socket?: string | null;
  user: string;
  database?: string | null;
}

export interface SessionInfo {
  id: string;
  spec: ConnectionSpec;
  identity: string;
  displayName: string;
  serverVersion: string;
  database: string | null;
}

export interface RecentConnection {
  identity: string;
  spec: ConnectionSpec;
  lastUsed: string;
}

export type LaunchEvent =
  | { type: "connect"; requestId: string; spec: ConnectionSpec; hasPassword: boolean; savePassword: boolean }
  | { type: "error"; message: string };

export type ErrorKind =
  | "database"
  | "connection"
  | "auth_failed"
  | "password_required"
  | "timeout"
  | "cancelled"
  | "confirmation_required"
  | "session_not_found"
  | "invalid_input"
  | "keychain"
  | "io";

export interface AppError {
  kind: ErrorKind;
  message: string;
  code: string | null;
}

export type Cell =
  | { t: "null" }
  | { t: "number"; v: string }
  | { t: "text"; v: string }
  | { t: "temporal"; v: string }
  | { t: "json"; v: string }
  | { t: "bytes"; v: { len: number; hexPreview: string } };

export interface ColumnMeta {
  name: string;
  typeName: string;
}

export interface ResultSet {
  columns: ColumnMeta[];
  rows: Cell[][];
  totalRows: number;
  truncated: boolean;
  affectedRows: number;
  lastInsertId: number | null;
  returnsRows: boolean;
}

export interface ExecuteResponse {
  results: ResultSet[];
  elapsedMs: number;
  database: string | null;
}

export interface DatabaseInfo {
  name: string;
  isSystem: boolean;
}

export interface TableInfo {
  name: string;
  isView: boolean;
  rowsEstimate: number | null;
  sizeBytes: number | null;
  comment: string;
}

export interface ColumnInfo {
  name: string;
  columnType: string;
  nullable: boolean;
  default: string | null;
  key: string;
  extra: string;
  comment: string;
}

export interface IndexInfo {
  name: string;
  unique: boolean;
  columns: string[];
}

export interface TableStructure {
  columns: ColumnInfo[];
  indexes: IndexInfo[];
  ddl: string;
}

export interface BrowseRequest {
  database: string;
  table: string;
  offset: number;
  limit: number;
  orderBy?: string | null;
  descending?: boolean;
  filter?: string | null;
  /** Single-column primary key; enables ID shorthand in `filter`. */
  primaryKey?: string | null;
}

export interface AppliedFilter {
  kind: "ids" | "sql";
  sql: string;
  label: string;
}

export interface BrowsePage {
  result: ResultSet;
  hasMore: boolean;
  filter?: AppliedFilter | null;
}

/** Per-connection UI state, keyed by connection identity on the Rust side. */
export interface UiState {
  tables: Record<string, { widths: Record<string, number> }>;
  detailPaneWidth: number | null;
  inspectorWidth: number | null;
  bottomPaneHeight: number | null;
  sqlEditorHeight: number | null;
}

export type PaneSize = "detailPaneWidth" | "inspectorWidth" | "bottomPaneHeight" | "sqlEditorHeight";

export const EMPTY_UI_STATE: UiState = { tables: {}, detailPaneWidth: null, inspectorWidth: null, bottomPaneHeight: null, sqlEditorHeight: null };

export type FavoriteTarget =
  | { kind: "table"; database: string; table: string }
  | { kind: "query"; sql: string };

export interface Favorite {
  id: string;
  name: string;
  scope: string | null;
  target: FavoriteTarget;
}

export type NewFavorite = Omit<Favorite, "id">;

export interface HistoryEntry {
  account: string;
  database: string | null;
  sql: string;
  /** The stored SQL was cut to 64 KiB. */
  truncated: boolean;
  executedAt: string;
  elapsedMs: number;
  error: string | null;
}

// ---------- WordPress lens ----------

export interface ValueNode {
  kind: "null" | "bool" | "int" | "float" | "string" | "array" | "object" | "ref" | "enum" | "custom";
  text?: string;
  class?: string;
  children?: { key: string; keyKind: "int" | "string" | "public" | "protected" | "private"; node: ValueNode }[];
  note?: string;
}

export interface DecodedValue {
  format: "php" | "json";
  root: ValueNode | null;
  warning: string | null;
  error: string | null;
}

export type HposMode = "off" | "on" | "on_with_sync";

export interface WpContext {
  prefix: string;
  version: string | null;
  sites: { blogId: number; prefix: string; domain: string | null; path: string | null }[];
  wc: { version: string | null; hpos: HposMode; hasActionScheduler: boolean } | null;
  site: {
    name: string | null;
    url: string | null;
    currency: string | null;
    gmtOffsetSecs: number;
    theme: string | null;
    activePlugins: string[];
    uploadsUrl: string | null;
  };
}

export interface WpInfo {
  context: WpContext;
  entities: [string, string][];
  tableEntities: Record<string, { entity: string; idColumn: string }>;
}

export interface EntityLink {
  entity: string;
  id: string;
  label: string | null;
  image: string | null;
}

export type Hint =
  | { type: "money"; text: string }
  | { type: "time"; unix: number; utc: boolean }
  | { type: "flag"; on: boolean }
  | { type: "url"; href: string }
  | { type: "userAgent"; text: string }
  | { type: "email" }
  | { type: "roles"; roles: string[] }
  | { type: "country"; code: string }
  | { type: "guest" };

export type Sensitive = "secret";

export interface DocField {
  name: string;
  label: string | null;
  value: Cell;
  decoded: DecodedValue | null;
  link: EntityLink | null;
  hint: Hint | null;
  sensitive: Sensitive | null;
}

export interface EntityDoc {
  entity: string;
  label: string;
  id: string;
  title: string | null;
  subtitle: string | null;
  image: string | null;
  table: string;
  idColumn: string;
  fields: DocField[];
  /** Grouped by literal key prefix (`_ppcp*`); "Other" holds keys that share no prefix. */
  meta: { name: string; private: boolean; entries: DocField[] }[];
  /** Meta table and its entity-ID and key columns. */
  metaSource: { table: string; fk: string; key: string } | null;
  relations: { name: string; label: string; entity: string; count: number }[];
}

export interface OptionsOverview {
  table: string;
  totalCount: number;
  autoloadBytes: number;
  groups: { prefix: string; label: string; count: number; bytes: number; autoloadCount: number; autoloadBytes: number }[];
  largestAutoload: { name: string; bytes: number }[];
  transients: { name: string; siteWide: boolean; bytes: number; expiresAt: number | null; expired: boolean }[];
}

// ---------- WordPress mode ----------

export interface Facet {
  value: string;
  label: string;
  count: number;
}

export interface Page<T> {
  items: T[];
  hasMore: boolean;
  facets: Facet[];
  kinds: Facet[];
}

export interface ListQuery {
  status?: string | null;
  kind?: string | null;
  search?: string | null;
  offset?: number;
  limit?: number;
  /** Whitelisted per list on the Rust side (created, total, status, …). */
  sort?: string | null;
  descending?: boolean | null;
}

export interface OrderSummary {
  id: string;
  kind: string;
  status: string;
  total: string | null;
  gateway: string | null;
  gatewayTitle: string | null;
  email: string | null;
  name: string | null;
  customerId: string | null;
  created: number | null;
  itemCount: number;
}

export interface UserSummary {
  id: string;
  login: string;
  email: string | null;
  name: string | null;
  registered: number | null;
  roles: string[];
  orderCount: number;
  totalSpent: string | null;
  lastOrder: number | null;
  avatar: string | null;
}

export interface ProductSummary {
  id: string;
  name: string;
  status: string;
  kind: string | null;
  sku: string | null;
  price: string | null;
  stockStatus: string | null;
  stock: string | null;
  totalSales: number;
  variations: number;
  image: string | null;
}

export interface ActionSummary {
  id: string;
  hook: string;
  status: string;
  scheduled: number | null;
  lastAttempt: number | null;
  attempts: number;
  args: string | null;
  group: string | null;
  message: string | null;
}

export interface SessionSummary {
  key: string;
  expires: number | null;
  customerId: string | null;
  email: string | null;
  items: number;
  total: string | null;
}

export interface PostSummary {
  id: string;
  title: string | null;
  kind: string;
  status: string;
  author: string | null;
  date: number | null;
  modified: number | null;
}

export interface DayBucket {
  date: string;
  completed: number;
  processing: number;
  failed: number;
  other: number;
}

export interface InsightAction {
  label: string;
  kind: "navigate" | "change";
  list: string | null;
  status: string | null;
  change: ChangeRequest | null;
}

export interface Insight {
  level: "info" | "warning" | "danger";
  text: string;
  action: InsightAction | null;
}

export interface Dashboard {
  orders: { total: number; today: number; byStatus: Facet[]; days: DayBucket[]; granularity: "day" | "hour" } | null;
  recentOrders: OrderSummary[];
  autoloadBytes: number;
  autoloadHealthy: boolean;
  transientsExpired: number;
  actions: { total: number; byStatus: Facet[]; recentFailed: ActionSummary[]; byHook: HookFailures[]; overdue: number } | null;
  hposDrift: number | null;
  tables: { name: string; rows: number; bytes: number; category: string }[];
  insights: Insight[];
}

export interface BrowserSection {
  list: string;
  label: string;
  count: number;
  breakdown: Facet[];
}

export interface Address {
  name: string | null;
  company: string | null;
  lines: string[];
  country: string | null;
  email: string | null;
  phone: string | null;
}

export interface TimelineEvent {
  at: number;
  kind: string;
  title: string;
  detail: string | null;
  level: "info" | "warning" | "error";
  link: EntityLink | null;
  /** Full original text when the title is a summary (log lines). */
  raw?: string | null;
  /** The table row behind the event. */
  row?: { table: string; idColumn: string; id: string } | null;
}

export interface RelatedChip {
  label: string;
  link: EntityLink | null;
  count: number | null;
  target: string | null;
}

export interface OrderView {
  view: "order";
  doc: EntityDoc;
  kind: string;
  status: string;
  total: string | null;
  currency: string | null;
  created: number | null;
  paid: number | null;
  completed: number | null;
  customer: EntityLink | null;
  customerEmail: string | null;
  parent: EntityLink | null;
  billing: Address | null;
  shipping: Address | null;
  addressNotes: string[];
  items: {
    id: string;
    name: string;
    kind: string;
    quantity: string | null;
    total: string | null;
    product: EntityLink | null;
    sku: string | null;
    meta: [string, string][];
  }[];
  totals: [string, string][];
  payment: {
    gateway: string | null;
    gatewayTitle: string | null;
    transactionId: string | null;
    paypalOrderId: string | null;
    intent: string | null;
    mode: string | null;
    source: string | null;
    payerEmail: string | null;
    fees: { gross: string; fee: string; net: string; feeRatio: number } | null;
  } | null;
  attribution: {
    source: string | null;
    sourceType: string | null;
    referrer: string | null;
    entry: string | null;
    pages: string | null;
    sessions: string | null;
    device: string | null;
    userAgent: string | null;
    sessionStart: number | null;
    utm: [string, string][];
  } | null;
  timeline: TimelineEvent[];
  related: RelatedChip[];
}

export interface CustomerView {
  view: "customer";
  doc: EntityDoc;
  name: string;
  login: string | null;
  email: string | null;
  avatar: string | null;
  roles: string[];
  registered: number | null;
  lastActive: number | null;
  billing: Address | null;
  shipping: Address | null;
  orderCount: number;
  totalSpent: string | null;
  averageOrder: string | null;
  orders: OrderSummary[];
  subscriptions: OrderSummary[];
  tokens: { id: string; gateway: string; kind: string; isDefault: boolean; summary: string }[];
  session: EntityLink | null;
  related: RelatedChip[];
}

export interface ProductView {
  view: "product";
  doc: EntityDoc;
  name: string;
  status: string;
  kind: string | null;
  image: string | null;
  sku: string | null;
  price: string | null;
  regularPrice: string | null;
  salePrice: string | null;
  stockStatus: string | null;
  stock: string | null;
  totalSales: string | null;
  terms: [string, string[]][];
  attributes: [string, string[]][];
  variations: {
    id: string;
    attributes: [string, string][];
    sku: string | null;
    price: string | null;
    stockStatus: string | null;
    stock: string | null;
    image: string | null;
  }[];
  related: RelatedChip[];
}

export interface ActionView {
  view: "action";
  doc: EntityDoc;
  hook: string;
  status: string;
  scheduled: number | null;
  args: DecodedValue | null;
  group: string | null;
  logs: { at: number | null; message: string }[];
}

export interface SessionView {
  view: "session";
  doc: EntityDoc;
  key: string;
  expires: number | null;
  customer: EntityLink | null;
  items: number;
  total: string | null;
  email: string | null;
  value: DecodedValue | null;
}

export type EntityViewData =
  | OrderView
  | CustomerView
  | ProductView
  | ActionView
  | SessionView
  | ({ view: "generic" } & EntityDoc);

export type ChangeRequest =
  | { type: "setCell"; database: string; table: string; keyColumn: string; key: string; column: string; value: string | null }
  | { type: "deleteRow"; database: string; table: string; keyColumn: string; key: string }
  | { type: "duplicateRow"; database: string; table: string; keyColumn: string; key: string }
  | { type: "insertRow"; database: string; table: string; values: [string, string | null][] }
  | { type: "setMeta"; database: string; entity: string; id: string; key: string; value: string }
  | { type: "deleteMeta"; database: string; entity: string; id: string; key: string }
  | { type: "setOrderStatus"; database: string; id: string; status: string }
  | { type: "setAutoload"; database: string; option: string; on: boolean }
  | { type: "deleteExpiredTransients"; database: string }
  | { type: "deleteLogsBefore"; database: string; days: number; source?: string | null }
  | { type: "retryAction"; database: string; id: string }
  | { type: "cancelAction"; database: string; id: string };

export interface PlannedChange {
  description: string;
  statements: string[];
  undo: string[];
}

export type NewValue =
  | { kind: "string"; text: string }
  | { kind: "int"; text: string }
  | { kind: "float"; text: string }
  | { kind: "bool"; text: boolean }
  | { kind: "null" };

export interface SnapshotScope {
  entity: string | null;
  id: string | null;
  includeRelated: boolean;
  optionPattern: string | null;
  includeActions: boolean;
}

export interface SnapshotInfo {
  id: string;
  label: string;
  scope: SnapshotScope;
  created: string;
  paths: number;
}

export interface DiffEntry {
  path: string;
  before: string | null;
  after: string | null;
}

export interface CompareRow {
  path: string;
  a: string | null;
  b: string | null;
}

export interface HookFailures {
  hook: string;
  count: number;
  lastFailure: number | null;
  lastMessage: string | null;
}

export interface PeekFact {
  label: string;
  value: string;
  kind: "text" | "status" | "money" | "time";
}

export interface FindHit {
  rowId: string;
  label: string;
  entity: string | null;
  entityId: string | null;
  column: string;
}

export interface FindGroup {
  source: string;
  table: string;
  idColumn: string;
  count: number;
  hits: FindHit[];
}

export interface SearchHit {
  entity: string;
  id: string;
  title: string;
  subtitle: string | null;
  matched: string;
}
