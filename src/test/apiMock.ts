import { vi } from "vitest";
import type { api as realApi } from "../api";
// Must not import the store or "../api": they would load the real api module while it is being mocked.
import { EMPTY_UI_STATE, type BrowsePage, type Dashboard, type EntityViewData, type Keybindings, type Page, type SnapshotInfo, type TableStructure, type UiState } from "../api/types";
import { NOW } from "./now";
import { session } from "./session";
import { settings } from "./settings";

function emptyPage(): Page<never> {
  return { items: [], hasMore: false, facets: [], kinds: [] };
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
  return {
    getSettings: vi.fn().mockResolvedValue(settings()),
    updateSettings: vi.fn(async (s) => s),
    openLogFolder: vi.fn().mockResolvedValue(undefined),
    openUrl: vi.fn().mockResolvedValue(undefined),
    logError: vi.fn().mockResolvedValue(undefined),
    takeLaunchEvents: vi.fn().mockResolvedValue([]),
    listRecents: vi.fn().mockResolvedValue([]),
    removeRecent: vi.fn().mockResolvedValue(undefined),
    connect: vi.fn().mockResolvedValue(session),
    disconnect: vi.fn().mockResolvedValue(undefined),
    listDatabases: vi.fn().mockResolvedValue([]),
    listTables: vi.fn().mockResolvedValue([]),
    describeTable: vi.fn().mockResolvedValue({ columns: [], indexes: [], ddl: null } satisfies TableStructure),
    completionSchema: vi.fn().mockResolvedValue({}),
    useDatabase: vi.fn().mockResolvedValue(undefined),
    execute: vi.fn().mockResolvedValue({ results: [], elapsedMs: 0, database: "db" }),
    cancelQuery: vi.fn().mockResolvedValue(undefined),
    browseTable: vi.fn(
      async (_s, req): Promise<BrowsePage> => ({
        result: { columns: [], rows: [], totalRows: 0, truncated: false, affectedRows: 0, lastInsertId: null, returnsRows: true },
        hasMore: false,
        limit: req.limit,
        filter: null,
      }),
    ),
    listFavorites: vi.fn().mockResolvedValue([]),
    createFavorite: vi.fn(async (f) => ({ ...f, id: "new-id" })),
    updateFavorite: vi.fn(async (f) => f),
    removeFavorite: vi.fn().mockResolvedValue(undefined),
    reorderFavorites: vi.fn().mockResolvedValue(undefined),
    listHistory: vi.fn().mockResolvedValue([]),
    clearHistory: vi.fn().mockResolvedValue(undefined),
    wpDetect: vi.fn().mockResolvedValue(null),
    wpLoad: vi.fn().mockResolvedValue([]),
    wpExpand: vi.fn().mockResolvedValue([]),
    wpOptions: vi.fn().mockResolvedValue({ table: "wp_options", totalCount: 0, autoloadBytes: 0, groups: [], largestAutoload: [], transients: [] }),
    wpTransientCleanupSql: vi.fn().mockResolvedValue("-- No expired transients"),
    decodeValue: vi.fn().mockResolvedValue(null),
    wpDashboard: vi.fn().mockResolvedValue(emptyDashboard),
    wpBrowser: vi.fn().mockResolvedValue([]),
    wpOrders: vi.fn(async () => emptyPage()),
    wpUsers: vi.fn(async () => emptyPage()),
    wpProducts: vi.fn(async () => emptyPage()),
    wpActions: vi.fn(async () => emptyPage()),
    wpActionHistogram: vi.fn().mockResolvedValue([]),
    wpSessions: vi.fn(async () => emptyPage()),
    wpContent: vi.fn(async () => emptyPage()),
    wpView: vi.fn(
      async (_s, _d, entity, id): Promise<EntityViewData> => ({
        view: "generic",
        entity,
        label: entity,
        id,
        title: null,
        subtitle: null,
        image: null,
        table: "wp_posts",
        idColumn: "ID",
        fields: [],
        meta: [],
        metaSource: null,
        relations: [],
      }),
    ),
    wpOrderLogs: vi.fn().mockResolvedValue([]),
    wpSearch: vi.fn().mockResolvedValue([]),
    wpPeek: vi.fn().mockResolvedValue([]),
    wpFindValue: vi.fn().mockResolvedValue([]),
    wpFailedByHook: vi.fn().mockResolvedValue([]),
    wpOptionsGroup: vi.fn().mockResolvedValue([]),
    planChange: vi.fn(async (_s, change) => ({ description: `planned ${change.type}`, statements: [`-- ${change.type}`], undo: [`-- undo ${change.type}`] })),
    applyChanges: vi.fn().mockResolvedValue(1),
    editSerialized: vi.fn().mockResolvedValue("edited"),
    wpSnapshotTake: vi.fn(async (_s, _d, scope, label): Promise<SnapshotInfo> => ({ id: "snap-1", label, scope, created: new Date(NOW).toISOString(), paths: 0 })),
    wpSnapshots: vi.fn().mockResolvedValue([]),
    wpSnapshotDiff: vi.fn().mockResolvedValue([]),
    wpSnapshotDelete: vi.fn().mockResolvedValue(undefined),
    wpCompare: vi.fn().mockResolvedValue([]),
    stageStatus: vi.fn().mockResolvedValue({ enabled: true, running: true, error: null, url: "http://127.0.0.1:27184/v1/stage", registerCommand: "stage register http://127.0.0.1:27184/v1/stage --token token-123", lastRequest: null }),
    stageNewToken: vi.fn(async () => settings({ stage: { enabled: true, port: 27184, token: "token-new" } })),
    onStageCollect: vi.fn().mockResolvedValue(() => {}),
    onStageInfo: vi.fn().mockResolvedValue(() => {}),
    onStageAct: vi.fn().mockResolvedValue(() => {}),
    stageAnchors: vi.fn().mockResolvedValue(undefined),
    stageDone: vi.fn().mockResolvedValue(undefined),
    stageInfo: vi.fn().mockResolvedValue(undefined),
    onLaunch: vi.fn().mockResolvedValue(() => {}),
    onCommand: vi.fn().mockResolvedValue(() => {}),
    getKeybindings: vi.fn().mockResolvedValue({}),
    getUiState: vi.fn().mockResolvedValue(EMPTY_UI_STATE),
    setUiState: vi.fn().mockImplementation(async (_i: string, ui: UiState) => ui),
    setKeybindings: vi.fn().mockImplementation(async (b: Keybindings) => b),
  };
}
