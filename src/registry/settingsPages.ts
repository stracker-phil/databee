/** Settings pages by id and the search over their rows. Core and modules register their own; the Settings window and the Stage hook
 *  read them here. */
import { useSyncExternalStore, type ComponentType, type ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import type { Settings } from "../api";
import { ranked } from "../lib/rank";

export interface SettingsPageProps {
  s: Settings;
  set: (patch: Partial<Settings>) => void;
}

/** A row (or group) the search finds: `id` is its `anchor`, `title` its label. */
export interface SettingsRow {
  id: string;
  title: string;
  /** More words the search finds the row by. */
  keywords?: string[];
  /** The anchor to scroll to while the row isn't shown. */
  under?: string;
}

interface SettingsPage {
  id: string;
  group: string;
  label: string;
  icon: LucideIcon;
  intro?: ReactNode;
  /** A page that keeps its own state (a module's settings file) ignores the props. */
  Body: ComponentType<SettingsPageProps>;
  /** Position inside its group; pages without one follow, by label. */
  order?: number;
  /** More words the search finds the page by. */
  keywords?: string[];
  /** Every row and group the page shows, by anchor; a function for rows that follow other state. */
  rows?: SettingsRow[] | (() => SettingsRow[]);
}

/** Nav order of the known groups; any other group follows them, by name. */
const GROUP_ORDER = ["Interface", "Database", "System"];

const pages = new Map<string, SettingsPage>();
const listeners = new Set<() => void>();
let version = 0;

/** Adds a page, or replaces the one with the same id. */
export function registerSettingsPage(page: SettingsPage): void {
  pages.set(page.id, page);
  version += 1;
  listeners.forEach((notify) => notify());
}

function groupRank(group: string): number {
  const index = GROUP_ORDER.indexOf(group);
  return index === -1 ? GROUP_ORDER.length : index;
}

function compare(a: SettingsPage, b: SettingsPage): number {
  const byGroup = groupRank(a.group) - groupRank(b.group) || a.group.localeCompare(b.group);
  if (byGroup !== 0) return byGroup;
  const rank = (p: SettingsPage) => p.order ?? Number.MAX_SAFE_INTEGER;
  const byOrder = rank(a) - rank(b);
  if (byOrder !== 0) return byOrder;
  return a.label.localeCompare(b.label);
}

/** Every page in nav order: by group, then `order`, then label. */
export function settingsPages(): SettingsPage[] {
  return [...pages.values()].sort(compare);
}

export const pageRows = (page: SettingsPage): SettingsRow[] => (typeof page.rows === "function" ? page.rows() : (page.rows ?? []));

export const findRow = (page: SettingsPage, id: string | undefined) => pageRows(page).find((row) => row.id === id);

/** The named page, the page holding the named row, or the first when the name is absent or unknown. */
export function resolveSettingsPage(id: string | undefined): SettingsPage | undefined {
  const all = settingsPages();
  return all.find((p) => p.id === id) ?? all.find((p) => findRow(p, id)) ?? all[0];
}

/** A search hit: its page, and the row that matched, if any. */
export interface SettingsMatch {
  page: SettingsPage;
  row?: SettingsRow;
}

/** Every page and row matching `query`, best first; ties keep registration order. */
export function searchSettings(query: string): SettingsMatch[] {
  const entries = settingsPages().flatMap((page): SettingsMatch[] => [{ page }, ...pageRows(page).map((row) => ({ page, row }))]);
  return ranked(entries, query, ({ page, row }) =>
    row ? [row.title, (row.keywords ?? []).join(" ")] : [page.label, [page.group, ...(page.keywords ?? [])].join(" ")],
  );
}

/** The page list under its group headings. */
export function settingsGroups(): { group: string; pages: SettingsPage[] }[] {
  const groups: { group: string; pages: SettingsPage[] }[] = [];
  for (const page of settingsPages()) {
    const last = groups.at(-1);
    if (last?.group === page.group) last.pages.push(page);
    else groups.push({ group: page.group, pages: [page] });
  }
  return groups;
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => void listeners.delete(listener);
}

/** Re-renders the caller when a page is registered, so a module may register after the window opened. */
export function useSettingsPagesVersion(): number {
  return useSyncExternalStore(subscribe, () => version);
}
