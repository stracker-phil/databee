// Re-exports the IPC types generated from Rust (`bun run types`); everything defined below is frontend-only.
import type { StageKeys, UiState } from "./types.gen";

export type * from "./types.gen";

/** Text size bounds and defaults in px; Rust's `Settings::sanitized` clamps to the same range. */
export const TEXT_SIZES = {
  ui: { min: 10, max: 24, default: 13 },
  data: { min: 9, max: 28, default: 12 },
} as const;

export const ACCENTS = ["blue", "purple", "pink", "graphite"] as const;

/** Shortcut overrides by command id; `null` unbinds. Rust: the `Keybindings` alias (a map, so ts-rs can't export it). */
export type Keybindings = Record<string, string | null>;

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

export type PaneSize = "detailPaneWidth" | "inspectorWidth" | "bottomPaneHeight" | "sqlEditorHeight";

export const EMPTY_UI_STATE: UiState = { tables: {}, detailPaneWidth: null, inspectorWidth: null, bottomPaneHeight: null, sqlEditorHeight: null };
