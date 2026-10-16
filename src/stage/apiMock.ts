import { vi } from "vitest";
import type { stageApi } from "./api";
import type { StageSettings, StageStatus } from "./types";

const status: StageStatus = {
  enabled: true,
  running: true,
  error: null,
  url: "http://127.0.0.1:27184/v1/stage",
  registerCommand: "stage register http://127.0.0.1:27184/v1/stage --token token-123",
  lastRequest: null,
};

const settings: StageSettings = { enabled: true, port: 27184, token: "token-123" };

/** A `stageApi` where every call resolves to a valid default; override per test with mockResolvedValue. */
export function createStageApiMock(): typeof stageApi {
  return {
    status: vi.fn().mockResolvedValue(status),
    settings: vi.fn().mockResolvedValue(settings),
    updateSettings: vi.fn(async (s: StageSettings) => s),
    newToken: vi.fn().mockResolvedValue({ ...settings, token: "token-new" }),
    onCollect: vi.fn().mockResolvedValue(() => {}),
    onInfo: vi.fn().mockResolvedValue(() => {}),
    onAct: vi.fn().mockResolvedValue(() => {}),
    anchors: vi.fn().mockResolvedValue(undefined),
    done: vi.fn().mockResolvedValue(undefined),
    info: vi.fn().mockResolvedValue(undefined),
  };
}
