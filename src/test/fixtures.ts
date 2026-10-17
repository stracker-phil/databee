import type { BrowsePage, Cell, ColumnMeta, PlannedChange, ResultSet, Settings, TableInfo, Value } from "../api";
import { session } from "./session";
import { settings } from "./settings";

export { session, settings };
import { resetStore, type QueryTab, type Tab, type TableTab } from "../state/store";

export function table(name: string, patch: Partial<TableInfo> = {}): TableInfo {
  return { name, isView: false, rowsEstimate: 10, sizeBytes: 16384, comment: "", ...patch };
}

export const text = (v: string): Cell => ({ t: "text", v });
export const num = (v: string): Cell => ({ t: "number", v });

/** A plain text value without provenance; override `raw`, `kind`, `source`, `style` or `secondary`. */
export function value(patch: Partial<Value> = {}): Value {
  return { raw: text("x"), kind: { kind: "text" }, source: null, style: null, secondary: null, ...patch };
}

export const columnMeta =(name: string, typeName: string): ColumnMeta => ({ name, typeName, origin: null });

export function resultSet(patch: Partial<ResultSet> = {}): ResultSet {
  return {
    id: 1,
    columns: [columnMeta("ID", "BIGINT UNSIGNED"), columnMeta("post_title", "TEXT")],
    rows: [
      [num("1"), text("Hello world")],
      [num("2"), { t: "null" }],
    ],
    totalRows: 2,
    truncated: false,
    affectedRows: 0,
    lastInsertId: null,
    returnsRows: true,
    kinds: [],
    serialized: [],
    cellKinds: [],
    styles: [],
    ...patch,
  };
}

export function browsePage(patch: Partial<BrowsePage> = {}): BrowsePage {
  return { table: "wp_posts", result: resultSet(), keys: [], hasMore: false, limit: 500, filter: null, columnsFilter: null, ...patch };
}

export function queryTab(patch: Partial<QueryTab> = {}): QueryTab {
  return {
    id: "q1",
    kind: "query",
    sessionId: "s1",
    title: "Query 1",
    sql: "",
    running: false,
    activeResult: 0,
    address: { connection: session.slug, database: "db", target: { kind: "home" }, view: null, compare: null },
    nav: { back: [], forward: [] },
    ...patch,
  };
}

export function tableTab(patch: Partial<TableTab> = {}): TableTab {
  const table = patch.table ?? "wp_posts";
  return {
    id: "t1",
    kind: "table",
    sessionId: "s1",
    title: table,
    database: "db",
    table,
    view: "data",
    where: [],
    address: { connection: session.slug, database: "db", target: { kind: "table", table, filter: [] }, view: null, compare: null },
    nav: { back: [], forward: [] },
    ...patch,
  };
}

type PendingChange = PlannedChange & { sessionId: string };

export function pendingChange(patch: Partial<PendingChange> = {}): PendingChange {
  return { id: 1, description: "Set x", statements: ["UPDATE `wp_options` SET `option_value` = 'x'"], canUndo: true, sessionId: "s1", ...patch };
}

/** A store with one connected session showing the given tabs. */
export function connectedStore(opts: { tabs?: Tab[]; tables?: TableInfo[]; settings?: Partial<Settings> } = {}) {
  const tabs = opts.tabs ?? [queryTab()];
  resetStore({
    settings: settings(opts.settings),
    sessions: [session],
    activeSessionId: session.id,
    tabs,
    queryCounter: { [session.id]: 1 },
    activeTabId: tabs[0]?.id ?? null,
    schema: {
      [session.id]: {
        databases: [
          { name: "db", isSystem: false },
          { name: "mysql", isSystem: true },
        ],
        tables: { db: opts.tables ?? [table("wp_options"), table("wp_posts"), table("wp_postmeta")] },
      },
    },
  });
}
