/** What Stage reads about DataBee: the main window's view and panels, its sheets, and a short state. */
import type { StageInfoReply, StageRect, StageSurface } from "./types";
import { resolveSettingsPage, settingsPages } from "../registry/settingsPages";
import { selectActiveSession, selectActiveTab, selectWp, selectWpRoute, useStore } from "../state/store";

type Flat = StageInfoReply["state"];

function rectOf(element: Element | null): StageRect | null {
  if (!element) return null;
  const r = element.getBoundingClientRect();
  return r.width > 0 && r.height > 0 ? { x: r.x, y: r.y, width: r.width, height: r.height } : null;
}

/** What the main window shows: connect, db, or wp:<screen>. */
export function stageView(): string {
  const s = useStore.getState();
  if (s.sessions.length === 0) return "connect";
  if (s.mode === "wp" && selectWp(s)) {
    const route = selectWpRoute(s);
    return `wp:${route.screen === "list" ? route.list : route.screen === "entity" ? route.entity : route.screen}`;
  }
  return "db";
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
  return { inspector: useStore.getState().selectedCell ? "open" : "closed" };
}

export function stageState(): Flat {
  const s = useStore.getState();
  const session = selectActiveSession(s);
  const tab = selectActiveTab(s);
  const wp = selectWp(s);
  const route = selectWpRoute(s);
  const state: Flat = {
    view: stageView(),
    mode: s.mode,
    connections: s.sessions.length,
    connection: session?.identity ?? null,
    server: session?.serverVersion ?? null,
    database: session?.database ?? null,
    tabs: s.tabs.filter((t) => t.sessionId === session?.id).length,
    tab: tab ? `${tab.kind}: ${tab.title}` : null,
    inspector: s.selectedCell ? "open" : "closed",
    settings: settingsPage() ?? "closed",
    palette: s.paletteOpen ? "open" : "closed",
    dialog: s.dialog && s.dialog.type !== "settings" ? s.dialog.type : null,
    pendingChanges: s.pending.length,
  };
  if (tab?.kind === "table") state.table = `${tab.database}.${tab.table} (${tab.view})`;
  if (tab?.kind === "query") state.query = tab.running ? "running" : tab.error ? "failed" : tab.response ? "done" : "not run";
  if (wp) {
    state.site = wp.context.site.url;
    state.prefix = wp.context.prefix;
    state.woocommerce = wp.context.wc?.version ?? null;
    if (s.mode === "wp" && route.screen === "entity") state.entity = `${route.entity} ${route.id}`;
  }
  return state;
}
