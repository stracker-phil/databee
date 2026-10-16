// Re-exports the IPC types generated from Rust (`bun run types`); everything defined below is frontend-only.
import type { LogLevel, UiState } from "./types.gen";

export type * from "./types.gen";

/** Text size bounds and defaults in px; Rust's `Settings::sanitized` clamps to the same range. */
export const TEXT_SIZES = {
  ui: { min: 10, max: 24, default: 13 },
  data: { min: 9, max: 28, default: 12 },
} as const;

/** Bounds of the numeric settings. Rust's `Settings::sanitized` is the source of truth; keep these equal. */
export const SETTINGS_LIMITS = {
  rowLimit: { min: 10, max: 100_000 },
  queryTimeoutSecs: { min: 1, max: 3600 },
  maxCellChars: { min: 20, max: 100_000 },
  connectTimeoutSecs: { min: 1, max: 120 },
  // `default_port` is a u16; `sanitized` only replaces 0.
  defaultPort: { min: 1, max: 65_535 },
} as const;

/** Least to most detail, as the Logging page lists them. */
const LOG_LEVEL_RANK: Record<LogLevel, number> = { error: 0, warn: 1, info: 2, debug: 3, trace: 4 };
/** The `Record` makes the compiler check every Rust level is listed. */
export const LOG_LEVELS = (Object.keys(LOG_LEVEL_RANK) as LogLevel[]).sort((a, b) => LOG_LEVEL_RANK[a] - LOG_LEVEL_RANK[b]);

export const ACCENTS = ["blue", "purple", "pink", "graphite"] as const;

/** Shortcut overrides by command id; `null` unbinds. Rust: the `Keybindings` alias (a map, so ts-rs can't export it). */
export type Keybindings = Record<string, string | null>;

export type PaneSize = "detailPaneWidth" | "inspectorWidth" | "bottomPaneHeight" | "sqlEditorHeight";

export const EMPTY_UI_STATE: UiState = { tables: {}, detailPaneWidth: null, inspectorWidth: null, bottomPaneHeight: null, sqlEditorHeight: null };
