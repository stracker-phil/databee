import { vi } from "vitest";
import type { api as realApi } from "../api";
// Must not import the store or "../api": they would load the real api module while it is being mocked.
import { EMPTY_UI_STATE, type ActionPreview, type BrowsePage, type Comparison, type Coverage, type Document, type Keybindings, type Peek, type PlannedChange, type Project, type ProjectRules, type Readout, type Resolved, type RuleDraft, type RulesInfo, type SnapshotDiff, type SnapshotInfo, type TableProfile, type TableReferences, type TableStructure, type UiState, type WatchPoll } from "../api/types";
import { parseAddress } from "../lib/address";
import { session } from "./session";
import { settings } from "./settings";

function emptyProject(database: string): Project {
  return { connection: "local", database, prefix: null, network_prefix: null, presets: null, references: [], disabled: [], enabled: [], kinds: [], styles: [], icons: [], saved: [] };
}

/** A readout with nothing in it, as for a database without presets, pins or recent objects. */
function emptyReadout(database: string): Readout {
  const none = { items: [], error: null };
  return {
    address: { connection: session.slug, database, target: { kind: "home" }, view: null, compare: null },
    status: { host: session.spec.host, slug: session.slug, database, prefix: "", presets: [], serverVersion: session.serverVersion, values: none },
    anomalies: none,
    recentlyCreated: { items: [], entities: [], error: null },
    watched: none,
    pinned: none,
    recent: none,
  };
}

/** An `api` where every call resolves to a minimal valid value of its return type; override per test with mockResolvedValue. */
export function createApiMock(): typeof realApi {
  let nextChangeId = 1;
  const planned = (type: string): PlannedChange => ({ id: nextChangeId++, description: `planned ${type}`, statements: [`-- ${type}`], canUndo: true });
  return {
    getSettings: vi.fn().mockResolvedValue(settings()),
    updateSettings: vi.fn(async (s) => s),
    openLogFolder: vi.fn().mockResolvedValue(undefined),
    openUrl: vi.fn().mockResolvedValue(undefined),
    chooseSavePath: vi.fn().mockResolvedValue(null),
    logError: vi.fn().mockResolvedValue(undefined),
    takeLaunchEvents: vi.fn().mockResolvedValue([]),
    listRecents: vi.fn().mockResolvedValue([]),
    removeRecent: vi.fn().mockResolvedValue(undefined),
    renameConnectionSlug: vi.fn().mockResolvedValue(undefined),
    resolveAddress: vi.fn(async (_s, text): Promise<Resolved> => ({ address: parseAddress(text), exists: true })),
    connect: vi.fn().mockResolvedValue(session),
    disconnect: vi.fn().mockResolvedValue(undefined),
    listDatabases: vi.fn().mockResolvedValue([]),
    listTables: vi.fn().mockResolvedValue([]),
    describeTable: vi.fn().mockResolvedValue({ columns: [], indexes: [], primaryKey: [], foreignKeys: [], ddl: null } satisfies TableStructure),
    tableRules: vi.fn().mockResolvedValue({ table: "wp_posts", ruleName: "posts", icon: null, columns: [], meta: [], referencedBy: [], entities: [], actions: [], ddl: null } satisfies TableProfile),
    tableReferences: vi.fn().mockResolvedValue({ outgoing: [], incoming: [] } satisfies TableReferences),
    presetsList: vi.fn().mockResolvedValue([]),
    projectGet: vi.fn().mockImplementation(async (_sessionId: string, database: string) => emptyProject(database)),
    projectUpdate: vi.fn().mockImplementation(async (_sessionId: string, database: string) => emptyProject(database)),
    projectAccents: vi.fn().mockResolvedValue({}),
    projectSetAccent: vi.fn().mockImplementation(async (_s: string, _d: string, accent: string | null) => accent),
    projectExport: vi.fn().mockResolvedValue(""),
    projectExportTo: vi.fn().mockResolvedValue(undefined),
    projectRules: vi.fn().mockImplementation(async (_sessionId: string, database: string): Promise<ProjectRules> => ({ project: emptyProject(database), prefix: "wp_", suggestedPrefix: null, presets: [], foreignKeys: 0, changes: [], groups: [], tables: [] })),
    ruleDraft: vi.fn(async (_s, _d, provenance): Promise<RuleDraft> => ({ source: `${provenance.table}.${provenance.column}`, meta: null })),
    coveragePreview: vi.fn().mockResolvedValue({ total: 0, resolved: 0, zeroOrNull: 0, sampleUnresolved: [], timedOut: false } satisfies Coverage),
    referenceCoverage: vi.fn().mockResolvedValue({ total: 0, resolved: 0, zeroOrNull: 0, sampleUnresolved: [], timedOut: false } satisfies Coverage),
    document: vi.fn(async (_s, text): Promise<Document> => ({ address: parseAddress(text), entity: null, header: { values: [] }, groups: [], alternatives: [], actions: [] })),
    peek: vi.fn(async (_s, text): Promise<Peek> => ({ address: parseAddress(text), keyColumn: null, summary: { values: [] }, matches: null })),
    watchStart: vi.fn().mockResolvedValue("w1"),
    watchPoll: vi.fn().mockResolvedValue({ delta: { changed: [], added: [], removed: [] }, document: null } satisfies WatchPoll),
    watchStop: vi.fn().mockResolvedValue(undefined),
    compare: vi.fn(async (_s, text): Promise<Comparison> => {
      const address = parseAddress(text);
      const right = address.target.kind === "row" && address.compare ? { ...address, target: { ...address.target, key: address.compare }, compare: null } : address;
      const empty = (a: typeof address): Document => ({ address: { ...a, compare: null }, entity: null, header: { values: [] }, groups: [], alternatives: [], actions: [] });
      return { left: empty(address), right: empty(right), delta: { changed: [], added: [], removed: [] } };
    }),
    snapshotTake: vi.fn(async (_s, address, label): Promise<SnapshotInfo> => ({ id: "snap-1", label, connection: session.slug, database: parseAddress(address).database, address, created: 0, values: 0 })),
    snapshotList: vi.fn().mockResolvedValue([]),
    snapshotDiff: vi.fn(async (): Promise<SnapshotDiff> => ({ snapshot: { id: "snap-1", label: "before", connection: session.slug, database: "db", address: "", created: 0, values: 0 }, delta: { changed: [], added: [], removed: [] }, document: null })),
    snapshotDelete: vi.fn().mockResolvedValue(undefined),
    tableActions: vi.fn().mockResolvedValue([]),
    previewAction: vi.fn(async (_s, _address, action): Promise<ActionPreview> => ({ id: 1, description: `${action}: Update 1 row`, canUndo: true, rows: [], sql: [] })),
    queueActionPreview: vi.fn(async () => planned("action")),
    rulesInfo: vi.fn().mockResolvedValue({ prefix: "wp_", networkPrefix: "wp_", suggestedPrefix: null, appliedPresets: [], availablePresets: [], entities: [], tableEntities: {}, tableIcons: {} } satisfies RulesInfo),
    refreshSchema: vi.fn().mockResolvedValue(undefined),
    completionSchema: vi.fn().mockResolvedValue({}),
    useDatabase: vi.fn().mockResolvedValue(undefined),
    execute: vi.fn().mockResolvedValue({ results: [], elapsedMs: 0, database: "db", failure: null }),
    cancelQuery: vi.fn().mockResolvedValue(undefined),
    fetchCell: vi.fn().mockResolvedValue(""),
    valueCondition: vi.fn().mockResolvedValue(""),
    savedQueries: vi.fn().mockResolvedValue([]),
    saveQuery: vi.fn(async (_s, _d, title, sql, pin) => ({ id: "saved-1", title, sql, pin })),
    updateSavedQuery: vi.fn(async (_s, _d, query) => query),
    removeSavedQuery: vi.fn().mockResolvedValue(undefined),
    copyPresetQueryToProject: vi.fn(async (_s, _d, _preset, id) => ({ id: `copy-${id}`, title: id, sql: "SELECT 1", pin: null })),
    valueCounts: vi.fn().mockResolvedValue([]),
    narrowRefusal: vi.fn().mockResolvedValue(null),
    narrowQuery: vi.fn(async (_s, sql, column, value) => `${sql} WHERE ${column} ${value === null ? "IS NULL" : `= '${value}'`}`),
    browseTable: vi.fn(
      async (_s, req): Promise<BrowsePage> => ({
        table: req.table,
        result: { id: 1, columns: [], rows: [], totalRows: 0, truncated: false, affectedRows: 0, lastInsertId: null, returnsRows: true, kinds: [], serialized: [], cellKinds: [], styles: [] },
        keys: [],
        hasMore: false,
        limit: req.limit,
        filter: null,
        columnsFilter: null,
      }),
    ),
    tableCheck: vi.fn(async (_s, req) => ({ hashes: req.keys.map(() => "h0") })),
    tableCheckRows: vi.fn().mockResolvedValue([]),
    tableSelectSql:vi.fn().mockResolvedValue("SELECT * FROM `db`.`table` LIMIT 100;"),
    listHistory: vi.fn().mockResolvedValue([]),
    clearHistory: vi.fn().mockResolvedValue(undefined),
    readout: vi.fn(async (_s, database) => emptyReadout(database)),
    readoutWatch: vi.fn(async (_s, _d, source, path) => ({ id: "watch-1", table: source.table, column: source.column, row: source.row, path })),
    readoutUnwatch: vi.fn().mockResolvedValue(undefined),
    readoutWatchLabel: vi.fn().mockResolvedValue(undefined),
    rememberObject: vi.fn().mockResolvedValue(undefined),
    decodeValue: vi.fn().mockResolvedValue(null),
    paletteSearch: vi.fn().mockResolvedValue([]),
    planChange: vi.fn(async (_s, change) => planned(change.type)),
    discardChange: vi.fn().mockResolvedValue(undefined),
    applyChanges: vi.fn().mockResolvedValue({ batchId: 1, affected: 1, canUndo: true }),
    undoBatch: vi.fn().mockResolvedValue(1),
    editSerialized: vi.fn().mockResolvedValue("edited"),
    onLaunch: vi.fn().mockResolvedValue(() => {}),
    onCommand: vi.fn().mockResolvedValue(() => {}),
    onRemote: vi.fn().mockResolvedValue(() => {}),
    remoteReply: vi.fn().mockResolvedValue(undefined),
    getKeybindings: vi.fn().mockResolvedValue({}),
    getUiState: vi.fn().mockResolvedValue(EMPTY_UI_STATE),
    setUiState: vi.fn().mockImplementation(async (_i: string, ui: UiState) => ui),
    setKeybindings: vi.fn().mockImplementation(async (b: Keybindings) => b),
  };
}
