/** Settings pages by id. Core and modules register their own; the Settings window and the Stage hook read them here. */
import { useSyncExternalStore, type ComponentType, type ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import type { Settings } from "../api";

export interface SettingsPageProps {
  s: Settings;
  set: (patch: Partial<Settings>) => void;
}

interface SettingsSection {
  id: string;
  title: string;
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
  /** Sections the page renders, each scrolled to by its id and found by the nav's search. */
  sections?: SettingsSection[];
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

export const hasSection = (page: SettingsPage, id: string | undefined) => page.sections?.some((section) => section.id === id) ?? false;

/** The named page, the page holding the named section, or the first when the name is absent or unknown. */
export function resolveSettingsPage(id: string | undefined): SettingsPage | undefined {
  const all = settingsPages();
  return all.find((p) => p.id === id) ?? all.find((p) => hasSection(p, id)) ?? all[0];
}

/** A page in the nav; `section` when the search found the page by that section's title. */
export interface SettingsNavEntry {
  page: SettingsPage;
  section?: SettingsSection;
}

/** Pages whose label, nav group or a section title contains `query` (any case). */
function searchSettingsPages(query: string): SettingsNavEntry[] {
  const needle = query.trim().toLowerCase();
  const has = (text: string) => text.toLowerCase().includes(needle);
  return settingsPages().flatMap((page) => {
    if (has(page.label) || has(page.group)) return [{ page }];
    const section = page.sections?.find((candidate) => has(candidate.title));
    return section ? [{ page, section }] : [];
  });
}

/** The nav groups in nav order, filtered to `query` when one is given. */
export function settingsGroups(query = ""): { group: string; pages: SettingsNavEntry[] }[] {
  const groups: { group: string; pages: SettingsNavEntry[] }[] = [];
  for (const hit of searchSettingsPages(query)) {
    const last = groups.at(-1);
    if (last?.group === hit.page.group) last.pages.push(hit);
    else groups.push({ group: hit.page.group, pages: [hit] });
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
