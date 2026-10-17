import { vi } from "vitest";
import type { api as realApi } from "../api";
// Must not import the store or "../api": they would load the real api module while it is being mocked.
import { EMPTY_UI_STATE, type ActionHistogram, type BrowsePage, type Coverage, type Dashboard, type Document,type Keybindings, type Page, type Peek, type PlannedChange, type Project, type ProjectRules, type Resolved, type RuleDraft, type RulesInfo, type SnapshotInfo, type TableReferences, type TableStructure, type UiState } from "../api/types";
import { parseAddress } from "../lib/address";
import { NOW } from "./now";
import { session } from "./session";
import { settings } from "./settings";

function emptyPage(): Page<never> {
  return { items: [], hasMore: false, limit: 50, facets: [], kinds: [] };
}

function emptyProject(database: string): Project {
  return { connection: "local", database, prefix: null, network_prefix: null, presets: null, references: [], disabled: [], kinds: [], styles: [], saved: [] };
}

const emptyDashboard: Dashboard = {
  orders: null,
  recentOrders: [],
  autoloadBytes: 0,
  autoloadHealthy: true,
  transientsExpired: 0,
  actions: null,
  hposDrift: null,
  tables: [],
  insights: [],
};

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
    tableReferences: vi.fn().mockResolvedValue({ outgoing: [], incoming: [] } satisfies TableReferences),
    presetsList: vi.fn().mockResolvedValue([]),
    projectGet: vi.fn().mockImplementation(async (_sessionId: string, database: string) => emptyProject(database)),
    projectUpdate: vi.fn().mockImplementation(async (_sessionId: string, database: string) => emptyProject(database)),
    projectExport: vi.fn().mockResolvedValue(""),
    projectExportTo: vi.fn().mockResolvedValue(undefined),
    projectRules: vi.fn().mockImplementation(async (_sessionId: string, database: string): Promise<ProjectRules> => ({ project: emptyProject(database), prefix: "wp_", suggestedPrefix: null, presets: [], foreignKeys: 0, references: [], kinds: [], styles: [], tables: [] })),
    ruleDraft: vi.fn(async (_s, _d, provenance): Promise<RuleDraft> => ({ source: `${provenance.table}.${provenance.column}`, meta: null })),
    coveragePreview: vi.fn().mockResolvedValue({ total: 0, resolved: 0, zeroOrNull: 0, sampleUnresolved: [], timedOut: false } satisfies Coverage),
    referenceCoverage: vi.fn().mockResolvedValue({ total: 0, resolved: 0, zeroOrNull: 0, sampleUnresolved: [], timedOut: false } satisfies Coverage),
    document: vi.fn(async (_s, text): Promise<Document> => ({ address: parseAddress(text), entity: null, header: [], groups: [], alternatives: [], actions: [] })),
    peek: vi.fn().mockResolvedValue({ keyColumn: null, values: [] } satisfies Peek),
    tableActions: vi.fn().mockResolvedValue([]),
    planAction: vi.fn(async (_s, _address, action) => planned(action)),
    rulesInfo: vi.fn().mockResolvedValue({ prefix: "wp_", networkPrefix: "wp_", suggestedPrefix: null, appliedPresets: [], availablePresets: [], entities: [], tableEntities: {} } satisfies RulesInfo),
    refreshSchema: vi.fn().mockResolvedValue(undefined),
    completionSchema: vi.fn().mockResolvedValue({}),
    useDatabase: vi.fn().mockResolvedValue(undefined),
    execute: vi.fn().mockResolvedValue({ results: [], elapsedMs: 0, database: "db", failure: null }),
    cancelQuery: vi.fn().mockResolvedValue(undefined),
    fetchCell: vi.fn().mockResolvedValue(""),
    valueCondition: vi.fn().mockResolvedValue(""),
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
    tableSelectSql: vi.fn().mockResolvedValue("SELECT * FROM `db`.`table` LIMIT 100;"),
    listFavorites: vi.fn().mockResolvedValue([]),
    createFavorite: vi.fn(async (f) => ({ ...f, id: "new-id" })),
    updateFavorite: vi.fn(async (f) => f),
    removeFavorite: vi.fn().mockResolvedValue(undefined),
    reorderFavorites: vi.fn().mockResolvedValue(undefined),
    listHistory: vi.fn().mockResolvedValue([]),
    clearHistory: vi.fn().mockResolvedValue(undefined),
    wpDetect: vi.fn().mockResolvedValue(null),
    wpOptions: vi.fn().mockResolvedValue({ table: "wp_options", totalCount: 0, autoloadBytes: 0, groups: [], largestAutoload: [], transients: [] }),
    decodeValue: vi.fn().mockResolvedValue(null),
    wpDashboard: vi.fn().mockResolvedValue(emptyDashboard),
    wpBrowser: vi.fn().mockResolvedValue([]),
    wpOrders: vi.fn(async () => emptyPage()),
    wpUsers: vi.fn(async () => emptyPage()),
    wpProducts: vi.fn(async () => emptyPage()),
    wpActions: vi.fn(async () => emptyPage()),
    wpActionHistogram: vi.fn().mockResolvedValue({ days: [], labels: { completed: "complete", processing: "in-progress", failed: "failed", other: "other" } } satisfies ActionHistogram),
    wpSessions: vi.fn(async () => emptyPage()),
    wpContent: vi.fn(async () => emptyPage()),
    paletteSearch: vi.fn().mockResolvedValue([]),
    wpFailedByHook: vi.fn().mockResolvedValue([]),
    wpOptionsGroup: vi.fn().mockResolvedValue([]),
    planChange: vi.fn(async (_s, change) => planned(change.type)),
    wpPlanChange: vi.fn(async (_s, change) => planned(change.type)),
    discardChange: vi.fn().mockResolvedValue(undefined),
    applyChanges: vi.fn().mockResolvedValue({ batchId: 1, affected: 1, canUndo: true }),
    undoBatch: vi.fn().mockResolvedValue(1),
    editSerialized: vi.fn().mockResolvedValue("edited"),
    wpSnapshotTake: vi.fn(async (_s, _d, scope, label): Promise<SnapshotInfo> => ({ id: "snap-1", label, scope, created: new Date(NOW).toISOString(), paths: 0 })),
    wpSnapshots: vi.fn().mockResolvedValue([]),
    wpSnapshotDiff: vi.fn().mockResolvedValue([]),
    wpSnapshotDelete: vi.fn().mockResolvedValue(undefined),
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
