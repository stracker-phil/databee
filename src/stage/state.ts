/** What Stage hands to Claude for narration: what databee shows now. Small, flat, no SQL, no secrets. */
import { allCommands } from "../commands";
import { selectActiveSession, selectActiveTab, selectWp, selectWpRoute, useStore } from "../state/store";

type Flat = Record<string, string | number | boolean | null>;

/** Where the user is: connect, db, wp:<screen>, or the dialog on top (settings:<page>, palette). */
export function stageView(): string {
  const s = useStore.getState();
  if (s.paletteOpen) return "palette";
  if (s.dialog?.type === "settings") return document.querySelector(".settings-pane[data-stage]")?.getAttribute("data-stage") ?? "settings";
  if (s.dialog) return s.dialog.type;
  if (s.sessions.length === 0) return "connect";
  if (s.mode === "wp" && selectWp(s)) {
    const route = selectWpRoute(s);
    return `wp:${route.screen === "list" ? route.list : route.screen === "entity" ? route.entity : route.screen}`;
  }
  return "db";
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
    inspector: s.selectedCell !== null,
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

export function stageCommands() {
  return allCommands().map((c) => ({ id: c.id, help: c.title, available: c.when?.() !== false }));
}
