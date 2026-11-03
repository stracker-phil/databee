import { api } from "../api";
import { selectActivePending, selectActiveSession, selectActiveTab, selectCanNav, selectCanUndo, useStore, type Tab } from "../state/store";
import { allCommands, bindingFor, bindingLabel, getCommand, isRecordingTarget, registerCommands, resolveKeydown, setOverrides } from "./registry";
import { activeScopes, registerScope } from "./scopes";
import { catalogCommands, type Handler } from "./catalog";
import { COLUMN_COMMANDS } from "./columns";
import { LIVE_COMMANDS } from "./live";
import { OBJECT_VALUE_COMMANDS, VALUE_COMMANDS } from "./values";
import { ROW_COMMANDS } from "./rows";

export * from "./registry";
export * from "./scopes";
export { EDIT_VALUE_EVENT, watchValue } from "./values";
export { commandItem } from "./menuItems";
export { paneMenu } from "./columns";

/** Window events for commands whose target lives inside a component. */
export const EVENTS = {
  focusWhere: "databee:focus-where",
  focusTableFilter: "databee:focus-table-filter",
  runQuery: "databee:run-query",
} as const;

function activeTabIs(kind: Tab["kind"]): boolean {
  return selectActiveTab(useStore.getState())?.kind === kind;
}

/** Registers the core scopes; safe to call again (tests and re-installs do). */
export function registerCoreScopes() {
  registerScope({ id: "global", parent: null, label: "Everywhere", isActive: () => true });
  registerScope({ id: "db", parent: "global", label: "Database", isActive: () => true });
  registerScope({ id: "table", parent: "db", label: "Table", isActive: () => activeTabIs("table") });
  registerScope({ id: "editor", parent: "db", label: "Editor", isActive: () => activeTabIs("query") });
  registerScope({ id: "object", parent: "global", label: "Object", isActive: () => activeTabIs("object") });
}

/** `?` opens the overview of the scopes active now; pressed again it closes it. */
function toggleShortcuts() {
  const s = useStore.getState();
  if (s.dialog?.type === "shortcuts") return s.closeDialog();
  if (!s.dialog) s.openDialog({ type: "shortcuts", scopes: activeScopes() });
}

function withQueryTab(run: (tab: Extract<Tab, { kind: "query" }>) => unknown) {
  const tab = selectActiveTab(useStore.getState());
  if (tab?.kind === "query") return run(tab);
}

function showObjectView(view: "document" | "rows") {
  const s = useStore.getState();
  const tab = selectActiveTab(s);
  if (tab?.kind !== "object") return;
  const column = tab.columns[tab.active];
  if (column?.kind === "object" && column.view !== view) s.toggleObjectView(tab.id);
}

const inResults = () => !!document.activeElement?.closest(".results [role='grid']");
const hasDatabase = () => !!selectActiveSession(useStore.getState())?.database;

const APP_HANDLERS: Handler[] = [
  { id: "palette.open", run: () => useStore.getState().setPaletteOpen(true) },
  { id: "palette.openTable", run: () => useStore.getState().setPaletteOpen(true, "t:") },
  // Without history the keys stay with the editor (⌘[ outdents there).
  { id: "nav.back", when: () => selectCanNav(useStore.getState(), "back"), run: () => useStore.getState().navBack() },
  { id: "nav.forward", when: () => selectCanNav(useStore.getState(), "forward"), run: () => useStore.getState().navForward() },
  {
    id: "view.sidebar",
    when: () => !!selectActiveSession(useStore.getState()),
    run: () => useStore.getState().toggleSidebar(),
  },
  { id: "view.refresh", run: () => void useStore.getState().refreshView() },
  { id: "help.shortcuts", run: toggleShortcuts },
  { id: "table.focusFilter", run: () => void window.dispatchEvent(new Event(EVENTS.focusWhere)) },
  {
    id: "table.filterMode",
    run: () => {
      const tab = selectActiveTab(useStore.getState());
      if (tab?.kind === "table") useStore.getState().toggleFilterMode(tab.id);
    },
  },
  { id: "sidebar.focusFilter", run: () => void window.dispatchEvent(new Event(EVENTS.focusTableFilter)) },
  {
    id: "tabs.new",
    when: () => useStore.getState().sessions.length > 0,
    run: () => useStore.getState().openQueryTab(),
  },
  {
    id: "tabs.close",
    run: () => {
      const s = useStore.getState();
      const tab = selectActiveTab(s);
      if (tab) s.closeTab(tab.id);
      else void api.runCommand("window.hide");
    },
  },
  { id: "tabs.next", run: () => useStore.getState().cycleTab(1) },
  { id: "tabs.previous", run: () => useStore.getState().cycleTab(-1) },
  { id: "editor.run", run: () => void window.dispatchEvent(new CustomEvent(EVENTS.runQuery, { detail: "statement" })) },
  { id: "editor.runAll", run: () => void window.dispatchEvent(new CustomEvent(EVENTS.runQuery, { detail: "all" })) },
  { id: "object.viewDocument", run: () => showObjectView("document") },
  { id: "object.viewRows", run: () => showObjectView("rows") },
  { id: "readout.open", when: hasDatabase, run: () => useStore.getState().openReadout() },
  { id: "project.open", when: hasDatabase, run: () => useStore.getState().openProject() },
  { id: "connection.new", run: () => useStore.getState().openDialog({ type: "connect" }) },
  { id: "connection.switch", run: () => useStore.getState().setPaletteOpen(true, "c:") },
  {
    id: "connection.retry",
    when: () => {
      const s = useStore.getState();
      return !!s.activeSessionId && s.health[s.activeSessionId]?.state === "down";
    },
    run: () => {
      const id = useStore.getState().activeSessionId;
      if (id) void useStore.getState().retryConnection(id);
    },
  },
  {
    id: "query.save",
    when: () => selectActivePending(useStore.getState()).length === 0,
    run: () => void withQueryTab((tab) => useStore.getState().saveQueryTab(tab.id)),
  },
  {
    id: "query.pin",
    when: inResults,
    run: () => void withQueryTab((tab) => useStore.getState().pinQueryTab(tab.id)),
  },
  { id: "changes.apply", when: () => selectActivePending(useStore.getState()).length > 0, run: () => void useStore.getState().applyChanges() },
  { id: "changes.undo", when: () => selectCanUndo(useStore.getState()), run: () => void useStore.getState().undoLast() },
  ...VALUE_COMMANDS,
  ...ROW_COMMANDS,
  ...OBJECT_VALUE_COMMANDS,
  ...COLUMN_COMMANDS,
  ...LIVE_COMMANDS,
];

/**
 * Registers the core scopes and the app's commands (again after `clearCommands`) and syncs user overrides
 * from the store. Module scopes must be registered first, or their commands throw.
 */
export function installCommands() {
  registerCoreScopes();
  if (allCommands().length === 0) registerCommands(catalogCommands(APP_HANDLERS));
  setOverrides(useStore.getState().keybindings);
}

/** The key label of a command (`⌘T`), or "" when unbound; follows the user's rebinding. */
export function useBindingLabel(id: string): string {
  const custom = useStore((s) => s.keybindings);
  return bindingLabel(bindingFor(id, custom));
}

/** Runs a command by id (native menu items and tests use this). */
export function runCommand(id: string): boolean {
  const c = getCommand(id);
  if (!c || c.when?.() === false) return false;
  void c.run();
  return true;
}

/** While a dialog or the palette is open, only these run; other keys belong to the overlay. */
const MODAL_ALLOWED = new Set(["settings.open", "help.shortcuts", "view.zoomIn", "view.zoomOut", "view.zoomReset"]);

/** Commands that keep firing while their key is held. */
const REPEATABLE = new Set(["view.zoomIn", "view.zoomOut", "nav.back", "nav.forward", "tabs.next", "tabs.previous", "object.down", "object.downArrow", "object.up", "object.upArrow"]);

function blockedByOverlay(id: string): boolean {
  const s = useStore.getState();
  return (s.dialog !== null || s.paletteOpen) && !MODAL_ALLOWED.has(id);
}

/** Runs a menu item or `databee command` id: the command, or the one sharing its menu item, in an active scope. */
export function runMenuCommand(id: string): boolean {
  const scopes = activeScopes();
  const target = allCommands().find((c) => (c.id === id || c.menu === id) && scopes.includes(c.scope) && c.when?.() !== false);
  if (!target || blockedByOverlay(target.id)) return false;
  void target.run();
  return true;
}

/** Capture-phase keydown handler for the window. */
export function handleKeydown(e: KeyboardEvent) {
  if (isRecordingTarget(e.target)) return;
  const c = resolveKeydown(e, activeScopes());
  if (!c || blockedByOverlay(c.id)) return;
  e.preventDefault();
  e.stopPropagation();
  // A held key is swallowed rather than passed on, so the native menu accelerator does not repeat it either.
  if (e.repeat && !REPEATABLE.has(c.id)) return;
  void c.run();
}
