import { useSyncExternalStore } from "react";
import { SCHEMES, type SchemeId, type Settings } from "../api";

/** Padding in px above and below a row's line, by row-padding step (1 to 4). */
export const ROW_PADDING_PX: Record<number, number> = { 1: 1, 2: 3, 3: 5, 4: 8 };

/** Padding in px for a row-padding step; out-of-range steps (hand-edited settings) get the default step. */
export function rowPaddingPx(step: number): number {
  return ROW_PADDING_PX[step] ?? ROW_PADDING_PX[2];
}

// One query for the whole app; matchMedia is missing in some test environments.
const darkQuery = typeof window.matchMedia === "function" ? window.matchMedia("(prefers-color-scheme: dark)") : null;

function subscribeToSystemTheme(onChange: () => void) {
  darkQuery?.addEventListener("change", onChange);
  return () => darkQuery?.removeEventListener("change", onChange);
}

const systemIsDark = () => darkQuery?.matches ?? false;

export function useSystemDark(): boolean {
  return useSyncExternalStore(subscribeToSystemTheme, systemIsDark);
}

type SchemeSettings = Pick<Settings, "theme" | "lightScheme" | "darkScheme">;

const DEFAULT_SCHEMES: SchemeSettings = { theme: "system", lightScheme: "ayu-light", darkScheme: "ayu-mirage" };

export interface ResolvedScheme {
  id: SchemeId;
  dark: boolean;
}

/** The scheme that applies: the dark one when the theme (or, for `system`, the OS) is dark. */
export function resolveScheme(settings: SchemeSettings, systemDark: boolean): ResolvedScheme {
  const dark = settings.theme === "system" ? systemDark : settings.theme === "dark";
  const wanted = dark ? settings.darkScheme : settings.lightScheme;
  const scheme = SCHEMES.find((s) => s.id === wanted) ?? SCHEMES.find((s) => s.dark === dark) ?? SCHEMES[0];
  return { id: scheme.id, dark: scheme.dark };
}

/** The applied scheme, for code that can't read CSS tokens (CodeMirror's light/dark base). */
export function useResolvedScheme(settings: SchemeSettings | null): ResolvedScheme {
  const systemDark = useSystemDark();
  return resolveScheme(settings ?? DEFAULT_SCHEMES, systemDark);
}

/** A family name the user typed, quoted for CSS and put in front of the bundled stack. */
function fontStack(family: string | null, bundled: string): string | null {
  if (!family) return null;
  return `"${family.replace(/["\\]/g, "\\$&")}", var(${bundled})`;
}

function setOrRemove(style: CSSStyleDeclaration, name: string, value: string | null) {
  if (value) style.setProperty(name, value);
  else style.removeProperty(name);
}

/** Applies appearance settings to <html> so CSS tokens pick them up. */
export function applyAppearance(settings: Settings, systemDark = systemIsDark()) {
  const root = document.documentElement;
  root.dataset.scheme = resolveScheme(settings, systemDark).id;
  root.dataset.accent = settings.accent;
  root.style.setProperty("--pad", `${rowPaddingPx(settings.rowPadding)}px`);
  root.style.setProperty("--ui-font-size", `${settings.uiFontSize}px`);
  root.style.setProperty("--data-font-size", `${settings.dataFontSize}px`);
  setOrRemove(root.style, "--font-ui", fontStack(settings.fontUi, "--font-ui-bundled"));
  setOrRemove(root.style, "--font-mono", fontStack(settings.fontMono, "--font-mono-bundled"));
}

/** Applies the default scheme for the OS appearance before settings load, so the first paint has colors. */
export function applyInitialScheme() {
  document.documentElement.dataset.scheme = resolveScheme(DEFAULT_SCHEMES, systemIsDark()).id;
}

// The virtualizer needs the row height in px; CSS owns it as `--row-h`.
// Measured on each appearance change; layout-less environments (tests) keep the default.
const DEFAULT_ROW_HEIGHT = 26;
let rowHeight = DEFAULT_ROW_HEIGHT;
const rowHeightListeners = new Set<() => void>();

function measureRowHeight(): number {
  const probe = document.createElement("div");
  probe.style.cssText = "position: absolute; visibility: hidden; height: var(--row-h)";
  document.body.appendChild(probe);
  const height = probe.getBoundingClientRect().height;
  probe.remove();
  return height > 0 ? Math.round(height) : DEFAULT_ROW_HEIGHT;
}

function publishRowHeight(next: number) {
  if (next === rowHeight) return;
  rowHeight = next;
  rowHeightListeners.forEach((listener) => listener());
}

function subscribeToRowHeight(onChange: () => void) {
  rowHeightListeners.add(onChange);
  return () => {
    rowHeightListeners.delete(onChange);
  };
}

/** Re-reads `--row-h` after `applyAppearance` and notifies `useRowHeight` users when it changed. */
export function syncRowHeight() {
  publishRowHeight(measureRowHeight());
}

export function useRowHeight(): number {
  return useSyncExternalStore(subscribeToRowHeight, () => rowHeight);
}
