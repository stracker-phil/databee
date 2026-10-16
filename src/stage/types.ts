// Re-exports the Stage types generated from Rust (`bun run types`). `StageWindow` and `StageActRequest` below
// mirror the JSON `stage/hooks.rs` builds for `stage:act` by hand.
import type { StageKeys } from "./types.gen";

export type * from "./types.gen";

/** The windows the Stage hook exposes; settings and the palette are sheets in the main one. */
export type StageWindow = "main" | "settings" | "palette";

/** Rust → page (`stage:act`), built as JSON in `stage::hooks`. With `action`, `window` names a sheet to open or close. */
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
