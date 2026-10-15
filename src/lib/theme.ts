import { useEffect, useState } from "react";
import type { Settings } from "../api";

/** Grid row height in px for the data size and density. CSS reads it as `--row-height`. */
export function rowHeightFor(settings: Pick<Settings, "dataFontSize" | "density">): number {
  const factor = settings.density === "compact" ? 1.75 : 2.1667;
  return Math.round(settings.dataFontSize * factor);
}

/** Applies appearance settings to <html> so CSS tokens pick them up. */
export function applyAppearance(settings: Settings) {
  const root = document.documentElement;
  if (settings.theme === "system") delete root.dataset.theme;
  else root.dataset.theme = settings.theme;
  root.dataset.accent = settings.accent;
  root.dataset.density = settings.density;
  root.style.setProperty("--ui-font-size", `${settings.uiFontSize}px`);
  root.style.setProperty("--data-font-size", `${settings.dataFontSize}px`);
  root.style.setProperty("--row-height", `${rowHeightFor(settings)}px`);
}

/** "light" or "dark", following the setting or the OS when set to system. */
export function useResolvedTheme(theme: Settings["theme"] | undefined): "light" | "dark" {
  const query = typeof window.matchMedia === "function" ? window.matchMedia("(prefers-color-scheme: dark)") : null;
  const [systemDark, setSystemDark] = useState(query?.matches ?? false);
  useEffect(() => {
    if (!query) return;
    const onChange = (e: MediaQueryListEvent) => setSystemDark(e.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, [query]);
  if (theme === "light" || theme === "dark") return theme;
  return systemDark ? "dark" : "light";
}
