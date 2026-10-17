/** What each tab kind does in the store; its icon, tooltip, Stage id and panel are in `components/tabViews.tsx`. */
import { objectColumnAt } from "../lib/columns";
import type { Store } from "./store";
import type { Tab, TabOfKind } from "./slices/tabs";

export interface TabKind<T extends Tab = Tab> {
  /** The database the tab's values and rules belong to; `column` names an object tab's stack column (default: the active one). */
  database: (tab: T, s: Store, column?: number) => string | null;
  /** A single click opens it in the session's preview tab, which a double click keeps. */
  preview: boolean;
}

export const TAB_KINDS: { [K in Tab["kind"]]: TabKind<TabOfKind<K>> } = {
  query: { database: (tab, s) => s.sessions.find((x) => x.id === tab.sessionId)?.database ?? null, preview: false },
  table: { database: (tab) => tab.database, preview: true },
  object: {
    database: (tab, _, column) => objectColumnAt(tab.columns, column ?? tab.active)?.address.database ?? tab.address.database,
    preview: true,
  },
  project: { database: (tab) => tab.database, preview: false },
  readout: { database: (tab) => tab.database, preview: false },
};

export const tabKind = <T extends Tab>(tab: T): TabKind<T> => TAB_KINDS[tab.kind] as unknown as TabKind<T>;

/** The database `tab` works in: a table's, an object column's row, a project's or readout's own; a query tab's: its session's current one. */
export const tabDatabase = (s: Store, tab: Tab, column?: number): string | null => tabKind(tab).database(tab, s, column);
