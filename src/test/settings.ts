import type { Settings } from "../api";

export function settings(patch: Partial<Settings> = {}): Settings {
  return {
    theme: "system",
    accent: "blue",
    uiFontSize: 13,
    dataFontSize: 12,
    density: "comfortable",
    rowLimit: 1000,
    queryTimeoutSecs: 30,
    confirmDestructive: true,
    nullLabel: "NULL",
    maxCellChars: 500,
    connectTimeoutSecs: 10,
    defaultPort: 3306,
    showSystemSchemas: false,
    logLevel: "info",
    screenShareMode: false,
    gravatar: false,
    ...patch,
  };
}
