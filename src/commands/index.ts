import { selectActiveTab, selectWp, selectWpRoute, useStore, type Store, type WpList, type WpRoute } from "../state/store";
import { getCommand, registerCommands, resolveKeydown, setOverrides, type Command, type Scope } from "./registry";

export * from "./registry";

/** Window events for commands whose target lives inside a component. */
export const EVENTS = {
  focusWhere: "databee:focus-where",
  focusTableFilter: "databee:focus-table-filter",
  runQuery: "databee:run-query",
} as const;

export function activeScopes(s: Store): Scope[] {
  const scopes: Scope[] = ["global"];
  if (s.mode === "wp" && selectWp(s)) {
    scopes.push("wp");
    if (selectWpRoute(s).screen === "entity") scopes.push("document");
  } else {
    scopes.push("db");
    const tab = selectActiveTab(s);
    if (tab?.kind === "table") scopes.push("table");
    if (tab?.kind === "query") scopes.push("editor");
  }
  return scopes;
}

const clamp = (n: number, min: number, max: number) => Math.min(max, Math.max(min, n));

function zoom(step: number | null) {
  const s = useStore.getState();
  if (!s.settings) return;
  const next =
    step === null
      ? { uiFontSize: 13, dataFontSize: 12 }
      : { uiFontSize: clamp(s.settings.uiFontSize + step, 10, 24), dataFontSize: clamp(s.settings.dataFontSize + step, 9, 28) };
  void s.saveSettings({ ...s.settings, ...next });
}

const inDb = () => useStore.getState().mode === "db" || !selectWp(useStore.getState());
const inWp = () => useStore.getState().mode === "wp" && !!selectWp(useStore.getState());
const hasWp = () => !!selectWp(useStore.getState());
const onEntity = () => inWp() && selectWpRoute(useStore.getState()).screen === "entity";

const go = (id: string, title: string, route: WpRoute): Command => ({
  id,
  title,
  scope: "wp",
  when: hasWp,
  run: () => useStore.getState().navigate(route),
});

const SCREENS: Command[] = [
  go("go.dashboard", "Dashboard", { screen: "dashboard" }),
  go("go.orders", "Orders", { screen: "list", list: "orders" }),
  go("go.failedOrders", "Failed orders", { screen: "list", list: "orders", params: { status: "wc-failed" } }),
  go("go.subscriptions", "Subscriptions", { screen: "list", list: "subscriptions" }),
  go("go.users", "Users", { screen: "list", list: "users" }),
  go("go.products", "Products", { screen: "list", list: "products" }),
  go("go.actions", "Scheduled actions", { screen: "list", list: "actions" }),
  go("go.failedActions", "Failed actions", { screen: "list", list: "actions", params: { status: "failed" } }),
  go("go.sessions", "Sessions", { screen: "list", list: "sessions" }),
  go("go.content", "Content", { screen: "list", list: "content" }),
  go("go.settings", "Options", { screen: "list", list: "settings" }),
  go("go.snapshots", "Snapshots & diff", { screen: "snapshots" }),
];

export const APP_COMMANDS: Command[] = [
  { id: "palette.open", title: "Command palette", scope: "global", defaultBinding: "Mod+K", run: () => useStore.getState().setPaletteOpen(true) },
  { id: "palette.openTable", title: "Open table or entity", scope: "global", defaultBinding: "Mod+P", run: () => useStore.getState().setPaletteOpen(true, "t:") },
  { id: "view.wp", title: "Switch to WordPress view", scope: "global", defaultBinding: "Mod+1", when: hasWp, run: () => useStore.getState().setMode("wp") },
  { id: "view.db", title: "Switch to Database view", scope: "global", defaultBinding: "Mod+2", run: () => useStore.getState().setMode("db") },
  { id: "nav.back", title: "Back", scope: "global", defaultBinding: "Mod+[", menu: true, when: inWp, run: () => useStore.getState().goBack() },
  { id: "nav.forward", title: "Forward", scope: "global", defaultBinding: "Mod+]", menu: true, when: inWp, run: () => useStore.getState().goForward() },
  { id: "view.zoomIn", title: "Zoom in", scope: "global", defaultBinding: "Mod+=", run: () => zoom(1) },
  { id: "view.zoomOut", title: "Zoom out", scope: "global", defaultBinding: "Mod+-", run: () => zoom(-1) },
  { id: "view.zoomReset", title: "Reset zoom", scope: "global", defaultBinding: "Mod+0", run: () => zoom(null) },
  { id: "view.refresh", title: "Refresh current view", scope: "global", defaultBinding: "Mod+R", run: () => useStore.getState().bumpData() },
  {
    id: "view.inspectorDock",
    title: "Toggle inspector position (right / bottom)",
    scope: "global",
    menu: true,
    run: () => {
      const s = useStore.getState();
      if (s.settings) void s.saveSettings({ ...s.settings, inspectorDock: s.settings.inspectorDock === "bottom" ? "right" : "bottom" }, { quiet: true });
    },
  },
  { id: "table.focusFilter", title: "Value filter (WHERE)", scope: "table", defaultBinding: "Mod+F", run: () => void window.dispatchEvent(new Event(EVENTS.focusWhere)) },
  { id: "sidebar.focusFilter", title: "Filter tables", scope: "db", defaultBinding: "Mod+Shift+F", run: () => void window.dispatchEvent(new Event(EVENTS.focusTableFilter)) },
  {
    id: "tabs.new",
    title: "New query tab",
    scope: "global",
    defaultBinding: "Mod+T",
    menu: true,
    when: () => useStore.getState().sessions.length > 0,
    run: () => {
      const s = useStore.getState();
      s.setMode("db");
      s.openQueryTab();
    },
  },
  {
    id: "tabs.close",
    title: "Close tab",
    scope: "global",
    defaultBinding: "Mod+W",
    menu: true,
    when: () => inDb() && !!selectActiveTab(useStore.getState()),
    run: () => {
      const s = useStore.getState();
      const tab = selectActiveTab(s);
      if (tab) s.closeTab(tab.id);
    },
  },
  { id: "tabs.next", title: "Next tab", scope: "db", defaultBinding: "Ctrl+Tab", run: () => useStore.getState().cycleTab(1) },
  { id: "tabs.previous", title: "Previous tab", scope: "db", defaultBinding: "Ctrl+Shift+Tab", run: () => useStore.getState().cycleTab(-1) },
  { id: "editor.run", title: "Run statement at cursor", scope: "editor", defaultBinding: "Mod+Enter", run: () => void window.dispatchEvent(new CustomEvent(EVENTS.runQuery, { detail: "statement" })) },
  { id: "editor.runAll", title: "Run all statements", scope: "editor", defaultBinding: "Mod+Shift+Enter", run: () => void window.dispatchEvent(new CustomEvent(EVENTS.runQuery, { detail: "all" })) },
  {
    id: "document.source",
    title: "Open source row",
    scope: "document",
    defaultBinding: "Mod+E",
    when: () => onEntity() && !!useStore.getState().entityMeta,
    run: () => {
      const meta = useStore.getState().entityMeta;
      if (meta) useStore.getState().openInDatabase(meta.table, `\`${meta.idColumn}\` = ${meta.id}`);
    },
  },
  {
    id: "wp.snapshot",
    title: "Take snapshot",
    scope: "document",
    defaultBinding: "Mod+Shift+S",
    when: onEntity,
    run: () => {
      const route = selectWpRoute(useStore.getState());
      if (route.screen === "entity") void useStore.getState().takeSnapshot(route.entity, route.id);
    },
  },
  { id: "connection.new", title: "New connection…", scope: "global", defaultBinding: "Mod+N", menu: true, run: () => useStore.getState().openDialog({ type: "connect" }) },
  { id: "settings.open", title: "Settings…", scope: "global", defaultBinding: "Mod+,", menu: true, run: () => useStore.getState().openDialog({ type: "settings" }) },
  { id: "favorites.add", title: "Add to favorites", scope: "global", defaultBinding: "Mod+D", menu: true, when: () => inDb() && !!selectActiveTab(useStore.getState()), run: () => useStore.getState().addCurrentTabToFavorites() },
  { id: "changes.apply", title: "Apply pending changes", scope: "global", defaultBinding: "Mod+S", menu: true, when: () => useStore.getState().pending.length > 0, run: () => void useStore.getState().applyChanges() },
  { id: "changes.undo", title: "Undo last apply", scope: "global", when: () => useStore.getState().applied.length > 0, run: () => void useStore.getState().undoLast() },
  ...SCREENS,
];

export const LIST_LABELS: Record<WpList, string> = {
  orders: "Orders",
  subscriptions: "Subscriptions",
  users: "Users",
  products: "Products",
  actions: "Scheduled actions",
  sessions: "Sessions",
  content: "Content",
  settings: "Options",
};

let installed = false;

/** Registers the app's commands and syncs user overrides from the store. */
export function installCommands() {
  if (!installed) {
    registerCommands(APP_COMMANDS);
    installed = true;
  }
  setOverrides(useStore.getState().keybindings);
}

/** Runs a command by id (native menu items and tests use this). */
export function runCommand(id: string): boolean {
  const c = getCommand(id);
  if (!c || c.when?.() === false) return false;
  void c.run();
  return true;
}

/** Capture-phase keydown handler for the window. */
export function handleKeydown(e: KeyboardEvent) {
  const c = resolveKeydown(e, activeScopes(useStore.getState()));
  if (!c) return;
  e.preventDefault();
  e.stopPropagation();
  void c.run();
}
