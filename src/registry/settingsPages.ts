/** Settings pages by id. Core and modules register their own; the dialog and the Stage hook read them here. */
import { useSyncExternalStore, type ComponentType, type ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import type { Settings } from "../api";

export interface SettingsPageProps {
  s: Settings;
  set: (patch: Partial<Settings>) => void;
}

export interface SettingsPage {
  id: string;
  group: string;
  label: string;
  icon: LucideIcon;
  intro?: ReactNode;
  /** A page that keeps its own state (a module's settings file) ignores the props. */
  Body: ComponentType<SettingsPageProps>;
  /** Position inside its group; pages without one follow, by label. */
  order?: number;
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

/** The named page, or the first when the name is absent or unknown. */
export function resolveSettingsPage(id: string | undefined): SettingsPage | undefined {
  const all = settingsPages();
  return all.find((p) => p.id === id) ?? all[0];
}

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

/** Re-renders the caller when a page is registered, so a module may register after the dialog opened. */
export function useSettingsPagesVersion(): number {
  return useSyncExternalStore(subscribe, () => version);
}
