import type { DatabaseInfo, Value, WpInfo } from "../api";
import { documentText, findValue } from "../lib/document";
import { documentKey, type DocumentEntry } from "./slices/objects";
import type { StructureEntry } from "./slices/session";
import type { WpRoute } from "./slices/wp";
import type { Store } from "./store";

export const structureKey = (sessionId: string, database: string, table: string) => `${sessionId}/${database}/${table}`;

export const selectStructure = (s: Store, sessionId: string, database: string, table: string): StructureEntry | undefined =>
  s.structures[structureKey(sessionId, database, table)];

export const selectActiveSession = (s: Store) => s.sessions.find((x) => x.id === s.activeSessionId) ?? null;

/** The sidebar is collapsed for the active connection (persisted in its UI state). */
export const selectSidebarHidden = (s: Store): boolean => {
  const session = selectActiveSession(s);
  return !!session && !!s.uiState[session.identity]?.sidebarHidden;
};

/** How grids and values show NULL and how much text a cell shows; the defaults match Rust's settings. */
export const selectNullLabel = (s: Store) => s.settings?.nullLabel ?? "NULL";
export const selectMaxCellChars = (s: Store) => s.settings?.maxCellChars ?? 500;

export const selectActiveTab = (s: Store) => s.tabs.find((t) => t.id === s.activeTabId) ?? null;

export const selectDocument = (s: Store, sessionId: string, text: string): DocumentEntry | undefined => s.documents[documentKey(sessionId, text)];

/**
 * The value the value commands and the status bar act on: the selected grid cell of the active tab, else
 * the focused value of an object tab's document.
 */
export const selectFocusedValue = (s: Store): Value | null => {
  const tab = selectActiveTab(s);
  if (!tab) return null;
  if (s.selectedCell?.tabId === tab.id && s.selectedCell.value) return s.selectedCell.value;
  if (tab.kind !== "object" || tab.view !== "document" || !tab.focus) return null;
  const document = s.documents[documentKey(tab.sessionId, documentText(tab.address))]?.document;
  return document ? findValue(document, tab.focus) : null;
};

/** Back or forward has somewhere to go: the active tab's history, or WordPress mode's own. */
export const selectCanNav = (s: Store, direction: "back" | "forward"): boolean => {
  if (isWpView(s)) return direction === "back" ? s.wpNav.index > 0 : s.wpNav.index < s.wpNav.stack.length - 1;
  return (selectActiveTab(s)?.nav[direction].length ?? 0) > 0;
};

/** Databases of the active session, without system schemas unless shown or current. */
export const selectVisibleDatabases = (s: Store): DatabaseInfo[] => {
  const session = selectActiveSession(s);
  if (!session) return [];
  const showSystem = s.settings?.showSystemSchemas ?? false;
  const databases = s.schema[session.id]?.databases ?? [];
  return databases.filter((d) => showSystem || !d.isSystem || d.name === session.database);
};

/** Pending edits of the active session; apply and undo work per session. */
export const selectActivePending = (s: Store) => s.pending.filter((p) => p.sessionId === s.activeSessionId);

export const selectLastBatch = (s: Store) => [...s.applied].reverse().find((a) => a.sessionId === s.activeSessionId);

/** The newest applied batch of the active session can be undone. */
export const selectCanUndo = (s: Store) => selectLastBatch(s)?.canUndo ?? false;

export const selectWpRoute = (s: Store): WpRoute => s.wpNav.stack[s.wpNav.index] ?? { screen: "dashboard" };

/** WordPress info for the active session's current database (undefined = not probed yet). */
export const selectWp = (s: Store): WpInfo | null | undefined => {
  const session = selectActiveSession(s);
  return session?.database ? s.wp[`${session.id}/${session.database}`] : null;
};

/** The WordPress view replaces the database view only while a WordPress install is detected. */
export const isWpView = (s: Store) => s.mode === "wp" && !!selectWp(s);
