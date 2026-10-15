import { vi } from "vitest";
import type { api as realApi } from "../api";
// Must not import the store: it would load the real api module while it is being mocked.
import { settings } from "./settings";

/** An `api` whose calls resolve to harmless defaults; override per test with mockResolvedValue. */
export function createApiMock(): typeof realApi {
  return {
    getSettings: vi.fn().mockResolvedValue(settings()),
    updateSettings: vi.fn(async (s) => s),
    openLogFolder: vi.fn().mockResolvedValue(undefined),
    logError: vi.fn().mockResolvedValue(undefined),
    takeLaunchEvents: vi.fn().mockResolvedValue([]),
    listRecents: vi.fn().mockResolvedValue([]),
    removeRecent: vi.fn().mockResolvedValue(undefined),
    forgetPassword: vi.fn().mockResolvedValue(undefined),
    connect: vi.fn(),
    disconnect: vi.fn().mockResolvedValue(undefined),
    listDatabases: vi.fn().mockResolvedValue([]),
    listTables: vi.fn().mockResolvedValue([]),
    describeTable: vi.fn(),
    completionSchema: vi.fn().mockResolvedValue({}),
    useDatabase: vi.fn().mockResolvedValue(undefined),
    execute: vi.fn(),
    cancelQuery: vi.fn().mockResolvedValue(undefined),
    browseTable: vi.fn(),
    listFavorites: vi.fn().mockResolvedValue([]),
    saveFavorite: vi.fn(async (f) => ({ ...f, id: f.id || "new-id" })),
    removeFavorite: vi.fn().mockResolvedValue(undefined),
    reorderFavorites: vi.fn().mockResolvedValue(undefined),
    listHistory: vi.fn().mockResolvedValue([]),
    clearHistory: vi.fn().mockResolvedValue(undefined),
    onLaunch: vi.fn().mockResolvedValue(() => {}),
    onMenu: vi.fn().mockResolvedValue(() => {}),
  };
}
