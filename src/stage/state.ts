/** What Stage reads about DataBee: the main window's view and panels, its sheets, and a short state. */
import { formatAddress } from "../lib/address";
import type { StageInfoReply, StageRect, StageSurface } from "./types";
import { resolveSettingsPage, settingsPages } from "../registry/settingsPages";
import { projectKey } from "../state/slices/project";
import { hasValueColumn, selectActiveSession, selectActiveTab, selectSidebarHidden, useStore, type ObjectTab, type Tab } from "../state/store";
import type { TabOfKind } from "../state/slices/tabs";

type Flat = StageInfoReply["state"];

function rectOf(element: Element | null): StageRect | null {
  if (!element) return null;
  const r = element.getBoundingClientRect();
  return r.width > 0 && r.height > 0 ? { x: r.x, y: r.y, width: r.width, height: r.height } : null;
}

/** What the main window shows: connect or db. */
export function stageView(): string {
  return useStore.getState().sessions.length === 0 ? "connect" : "db";
}

/** The Settings pages as the settings window's sections, in nav order. */
export function settingsSections(): { id: string; title: string }[] {
  return settingsPages().map(({ id, label }) => ({ id, title: label }));
}

/** The page Settings was last opened at from this window, while it shows; Stage doesn't see the window's own page changes. */
function settingsPage(): string | null {
  const { shown, page } = useStore.getState().settingsWindow;
  return shown ? (resolveSettingsPage(page)?.id ?? null) : null;
}

/** The Settings window is in front of the main one; Stage reaches no anchor inside it, so it covers the page. */
function settingsRect(): StageRect | null {
  return useStore.getState().settingsWindow.shown ? { x: 0, y: 0, width: window.innerWidth, height: window.innerHeight } : null;
}

/** The palette is a sheet in the main window and Settings a window of its own; Stage sees both as windows. */
export function stageSurfaces(): StageSurface[] {
  return [
    {
      id: "settings",
      title: "Settings",
      rect: settingsRect(),
      sections: settingsSections(),
      section: settingsPage(),
    },
    { id: "palette", title: "Command Palette", rect: rectOf(document.querySelector('[data-stage-window="palette"]')) },
  ];
}

export function stageUi(): StageInfoReply["ui"] {
  return { valueColumn: valueColumnState(), sidebar: sidebarState() };
}

/** A value column is open in the active tab: in an object tab's stack, or next to a table or query tab's grid. */
function valueColumnState(): "open" | "closed" {
  const tab = selectActiveTab(useStore.getState());
  if (tab?.kind === "object") return tab.columns.some((c) => c.kind === "value") ? "open" : "closed";
  return tab && hasValueColumn(tab) ? "open" : "closed";
}

/** The database sidebar; the connect screen has none. */
function sidebarState(): "open" | "closed" {
  const s = useStore.getState();
  return stageView() === "db" && !selectSidebarHidden(s) ? "open" : "closed";
}

/** The active column's address and view, and the stack's size (and whether it is maximized) when it holds more than one column. */
function objectState(tab: ObjectTab): string {
  const column = tab.columns[tab.active];
  const shown = column?.kind === "object" ? `${formatAddress(column.address)} (${column.view})` : `${formatAddress(tab.address)} (value)`;
  if (tab.columns.length < 2) return shown;
  return `${shown} · column ${tab.active + 1} of ${tab.columns.length}${tab.maximized ? " · maximized" : ""}`;
}

/** The active tab's line in the state, under its kind. */
const TAB_STATE: { [K in Tab["kind"]]: (tab: TabOfKind<K>) => string } = {
  table: (tab) => `${tab.database}.${tab.table} (${tab.view})`,
  object: objectState,
  project: (tab) => (tab.form ? `${tab.database} (${tab.form.type} form)` : tab.database),
  readout: (tab) => tab.database,
  query: (tab) => (tab.running ? "running" : tab.error ? "failed" : tab.response ? "done" : "not run"),
};

const tabState = (tab: Tab) => (TAB_STATE[tab.kind] as (tab: Tab) => string)(tab);

export function stageState(): Flat {
  const s = useStore.getState();
  const session = selectActiveSession(s);
  const tab = selectActiveTab(s);
  const info = session?.database ? s.rulesInfo[projectKey(session.id, session.database)] : undefined;
  const state: Flat = {
    view: stageView(),
    connections: s.sessions.length,
    connection: session?.identity ?? null,
    server: session?.serverVersion ?? null,
    database: session?.database ?? null,
    tabs: s.tabs.filter((t) => t.sessionId === session?.id).length,
    tab: tab ? `${tab.kind}: ${tab.title}` : null,
    valueColumn: valueColumnState(),
    sidebar: sidebarState(),
    settings: settingsPage() ?? "closed",
    palette: s.paletteOpen ? "open" : "closed",
    dialog: s.dialog?.type ?? null,
    pendingChanges: s.pending.length,
  };
  if (tab) state[tab.kind] = tabState(tab);
  if (info) {
    state.prefix = info.prefix;
    state.presets = info.appliedPresets.join(", ");
  }
  return state;
}
