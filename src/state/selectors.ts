import type { Address, DatabaseInfo, Value } from "../api";
import { DEFAULT_MAX_PANES } from "../lib/columns";
import { documentText, findValue } from "../lib/document";
import { documentKey, type DocumentEntry } from "./slices/objects";
import type { StructureEntry } from "./slices/session";
import type { Store } from "./store";

export const structureKey = (sessionId: string, database: string, table: string) => `${sessionId}/${database}/${table}`;

export const selectStructure = (s: Store, sessionId: string, database: string, table: string): StructureEntry | undefined =>
  s.structures[structureKey(sessionId, database, table)];

export const selectActiveSession = (s: Store) => s.sessions.find((x) => x.id === s.activeSessionId) ?? null;

/** The open session whose connection slug is `slug` (the `connection` of an address). */
export const selectSessionBySlug = (s: Store, slug: string) => s.sessions.find((x) => x.slug === slug);

export const noConnectionNamed = (slug: string) => `No open connection is named ${slug}`;

/** The sidebar is collapsed for the active connection (persisted in its UI state). */
export const selectSidebarHidden = (s: Store): boolean => {
  const session = selectActiveSession(s);
  return !!session && !!s.uiState[session.identity]?.sidebarHidden;
};

/** How grids and values show NULL and how much text a cell shows; the defaults match Rust's settings. */
export const selectNullLabel = (s: Store) => s.settings?.nullLabel ?? "NULL";
export const selectMaxCellChars = (s: Store) => s.settings?.maxCellChars ?? 500;
export const selectMaxPanes = (s: Store) => s.settings?.maxPanes ?? DEFAULT_MAX_PANES;

export const selectActiveTab = (s: Store) => s.tabs.find((t) => t.id === s.activeTabId) ?? null;

export const selectDocument = (s: Store, sessionId: string, text: string): DocumentEntry | undefined => s.documents[documentKey(sessionId, text)];

/**
 * The value the value commands and the status bar act on: the selected grid cell of the active tab, else
 * the active column of an object tab: its focused document value, or the value a value column shows (none once its row is gone).
 */
export const selectFocusedValue = (s: Store): Value | null => {
  const tab = selectActiveTab(s);
  if (!tab) return null;
  if (s.selectedCell?.tabId === tab.id && s.selectedCell.value) return s.selectedCell.value;
  if (tab.kind !== "object") return null;
  const column = tab.columns[tab.active];
  if (column?.kind === "value") return column.missing ? null : column.value;
  if (column?.kind !== "object" || column.view !== "document" || !column.focus) return null;
  const document = s.documents[documentKey(tab.sessionId, documentText(column.address))]?.document;
  return document ? findValue(document, column.focus) : null;
};

const NO_GROUPS: string[] = [];

/** The collapsed group keys (`collapseKey`) of documents in `owner`'s database, kept per connection. */
export const selectCollapsedGroups = (s: Store, owner: Address): string[] => {
  const identity = selectSessionBySlug(s, owner.connection)?.identity;
  return (identity ? s.uiState[identity]?.collapsedGroups?.[owner.database] : undefined) ?? NO_GROUPS;
};

/** Back or forward has somewhere to go in the active tab's history. */
export const selectCanNav = (s: Store, direction: "back" | "forward"): boolean => (selectActiveTab(s)?.nav[direction].length ?? 0) > 0;

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

/** The pending tray offers undo until it is dismissed. */
export const selectOffersUndo = (s: Store) => selectCanUndo(s) && !selectLastBatch(s)?.dismissed;
