import { useEffect, useState } from "react";
import type { Settings } from "../api";

/** Applies appearance settings to <html> so CSS tokens pick them up. */
export function applyAppearance(settings: Settings) {
  const root = document.documentElement;
  if (settings.theme === "system") delete root.dataset.theme;
  else root.dataset.theme = settings.theme;
  root.dataset.accent = settings.accent;
  root.dataset.density = settings.density;
  root.style.setProperty("--editor-font-size", `${settings.editorFontSize}px`);
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
