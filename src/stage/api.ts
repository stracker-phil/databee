import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { PageAnchor, StageActReply, StageActRequest, StageInfoReply, StageSettings, StageStatus } from "./types";

/** Stage's only bridge to its Rust plugin. Tests mock this module with `createStageApiMock()`. */
export const stageApi = {
  status: () => invoke<StageStatus>("plugin:stage|stage_status"),
  settings: () => invoke<StageSettings>("plugin:stage|stage_settings"),
  updateSettings: (settings: StageSettings) => invoke<StageSettings>("plugin:stage|stage_update_settings", { settings }),
  newToken: () => invoke<StageSettings>("plugin:stage|stage_new_token"),

  /** The hook (hooks.ts): Rust asks, the page answers. */
  onCollect: (handler: (request: string) => void): Promise<UnlistenFn> =>
    listen<{ request: string }>("stage:collect", (e) => handler(e.payload.request)),
  onInfo: (handler: (request: string) => void): Promise<UnlistenFn> =>
    listen<{ request: string }>("stage:info", (e) => handler(e.payload.request)),
  onAct: (handler: (request: StageActRequest) => void): Promise<UnlistenFn> =>
    listen<StageActRequest>("stage:act", (e) => handler(e.payload)),
  anchors: (request: string, page: string, anchors: PageAnchor[]) =>
    invoke<void>("plugin:stage|stage_anchors", { reply: { request, page, anchors } }),
  done: (reply: StageActReply) => invoke<void>("plugin:stage|stage_done", { reply }),
  info: (reply: StageInfoReply) => invoke<void>("plugin:stage|stage_info", { reply }),
};
