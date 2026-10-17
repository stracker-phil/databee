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

/** The page the dialog shows: the named one, or the first when the name is absent or unknown. */
function settingsPage(): string | null {
  const dialog = useStore.getState().dialog;
  if (dialog?.type !== "settings") return null;
  return resolveSettingsPage(dialog.page)?.id ?? null;
}

/** Settings and the palette are sheets in the main window; Stage sees them as windows. */
export function stageSurfaces(): StageSurface[] {
  return [
    {
      id: "settings",
      title: "Settings",
      rect: rectOf(document.querySelector('[data-stage-window="settings"]')),
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

/** The active column's address and view, and the stack's size when it holds more than one column. */
function objectState(tab: ObjectTab): string {
  const column = tab.columns[tab.active];
  const shown = column?.kind === "object" ? `${formatAddress(column.address)} (${column.view})` : `${formatAddress(tab.address)} (value)`;
  return tab.columns.length > 1 ? `${shown} · column ${tab.active + 1} of ${tab.columns.length}` : shown;
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
    dialog: s.dialog && s.dialog.type !== "settings" ? s.dialog.type : null,
    pendingChanges: s.pending.length,
  };
  if (tab) state[tab.kind] = tabState(tab);
  if (info) {
    state.prefix = info.prefix;
    state.presets = info.appliedPresets.join(", ");
  }
  return state;
}
