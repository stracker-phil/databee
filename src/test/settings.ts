import type { Settings } from "../api";

export function settings(patch: Partial<Settings> = {}): Settings {
  return {
    theme: "system",
    lightScheme: "ayu-light",
    darkScheme: "ayu-mirage",
    accent: "orange",
    uiFontSize: 13,
    dataFontSize: 12,
    rowPadding: 2,
    maxPanes: 2,
    fontUi: null,
    fontMono: null,
    rowLimit: 1000,
    queryTimeoutSecs: 30,
    liveIntervalSecs: 2,
    confirmDestructive: true,
    nullLabel: "NULL",
    maxCellChars: 500,
    connectTimeoutSecs: 10,
    defaultPort: 3306,
    showSystemSchemas: false,
    logLevel: "info",
    ...patch,
  };
}
