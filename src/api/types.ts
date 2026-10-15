// Mirrors the serialized Rust types in src-tauri/src.

export type Theme = "system" | "light" | "dark";
export type Density = "comfortable" | "compact";
export type LogLevel = "error" | "warn" | "info" | "debug" | "trace";

export interface Settings {
  theme: Theme;
  accent: string;
  editorFontSize: number;
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
}

export const ACCENTS = ["blue", "purple", "pink", "orange", "green", "graphite"] as const;

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
}

export interface BrowsePage {
  result: ResultSet;
  hasMore: boolean;
}

export type FavoriteTarget =
  | { kind: "table"; database: string; table: string }
  | { kind: "query"; sql: string };

export interface Favorite {
  id: string;
  name: string;
  scope: string | null;
  target: FavoriteTarget;
}

export interface HistoryEntry {
  account: string;
  database: string | null;
  sql: string;
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
}

export interface WpInfo {
  context: WpContext;
  entities: [string, string][];
  tableEntities: Record<string, { entity: string; idColumn: string }>;
}

export interface EntityLink {
  entity: string;
  id: string;
}

export interface DocField {
  name: string;
  value: Cell;
  decoded: DecodedValue | null;
  link: EntityLink | null;
}

export interface EntityDoc {
  entity: string;
  label: string;
  id: string;
  title: string | null;
  subtitle: string | null;
  table: string;
  idColumn: string;
  fields: DocField[];
  meta: { name: string; private: boolean; entries: DocField[] }[];
  relations: { name: string; label: string; entity: string; count: number }[];
}

export interface OptionsOverview {
  table: string;
  totalCount: number;
  autoloadBytes: number;
  groups: { prefix: string; count: number; bytes: number; autoloadCount: number; autoloadBytes: number }[];
  transients: { name: string; siteWide: boolean; bytes: number; expiresAt: number | null; expired: boolean }[];
}
