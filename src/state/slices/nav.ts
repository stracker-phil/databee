/**
 * Addresses of tabs and their back/forward history. A table tab's address is derived from its table and
 * structured filter; a free SQL filter is not addressable and leaves the address as it is.
 */
import { api, type Address, type ColumnEquals } from "../../api";
import { AddressError, formatAddress, parseAddress, sameAddress } from "../../lib/address";
import { fail } from "../requests";
import { isWpView, selectActiveTab, selectStructure } from "../selectors";
import type { Slice, Store } from "../store";
import type { Tab, TableTab } from "./tabs";

/** Entries kept per direction and tab. */
export const NAV_LIMIT = 50;

export interface TabNav {
  back: Address[];
  forward: Address[];
}

export const EMPTY_NAV: TabNav = { back: [], forward: [] };

/** What a table tab shows, as far as an address can say it. */
export interface TableLocation {
  database: string;
  table: string;
  where: ColumnEquals[];
}

export interface NavSlice {
  /** Opens an address given as text; errors become a toast. */
  openAddress(text: string): Promise<void>;
  /** Shows other rows in table tab `tabId`, recording the current address for back. */
  showRows(tabId: string, location: TableLocation): void;
  /** Applies a typed WHERE filter to table tab `tabId`; not addressable, so no history entry. */
  setTableFilter(tabId: string, filter: string): void;
  /** Back and forward in the active tab; in WordPress mode through `wpNav`. */
  navBack(): Promise<void>;
  navForward(): Promise<void>;
  /** Stores a renamed connection slug and rewrites the addresses of that connection's tabs. */
  renameConnectionSlug(identity: string, slug: string): Promise<void>;
}

const slugOf = (s: Store, sessionId: string) => s.sessions.find((x) => x.id === sessionId)?.slug ?? "";

function cachedPrimaryKey(s: Store, sessionId: string, database: string, table: string): string[] | null {
  return selectStructure(s, sessionId, database, table)?.structure?.primaryKey ?? null;
}

/** A filtered table says so, since the filter is not in the title. */
export function tableTitle(table: string, where: ColumnEquals[], filter?: string): string {
  return filter || where.length > 0 ? `${table} (filtered)` : table;
}

/** The key value when `where` is exactly one non-null match on the single-column primary key. */
function singleKeyMatch(where: ColumnEquals[], primaryKey: string[] | null): string | null {
  if (where.length !== 1 || primaryKey?.length !== 1) return null;
  const [{ column, value }] = where;
  return column.toLowerCase() === primaryKey[0].toLowerCase() ? value : null;
}

/** A single match on the single-column primary key is the row itself (shown as rows); anything else is the filtered table. */
export function tableAddress(s: Store, sessionId: string, { database, table, where }: TableLocation): Address {
  const connection = slugOf(s, sessionId);
  const key = singleKeyMatch(where, cachedPrimaryKey(s, sessionId, database, table));
  if (key !== null) return { connection, database, target: { kind: "row", table, key }, view: "rows", compare: null };
  return { connection, database, target: { kind: "table", table, filter: where }, view: null, compare: null };
}

/** The database home, the address of a query tab; none without a current database. */
export function homeAddress(s: Store, sessionId: string, database: string | null): Address | null {
  return database ? { connection: slugOf(s, sessionId), database, target: { kind: "home" }, view: null, compare: null } : null;
}

/** `nav` after leaving `from`; leaving an address for itself records nothing. */
export function pushed(nav: TabNav, from: Address, to: Address): TabNav {
  if (sameAddress(from, to)) return nav;
  return { back: [...nav.back, from].slice(-NAV_LIMIT), forward: [] };
}

/** The table location of a table or row address; a row needs the table's single-column primary key. */
async function locationOf(get: () => Store, sessionId: string, address: Address): Promise<TableLocation | null> {
  const { database, target } = address;
  if (target.kind === "table") return { database, table: target.table, where: target.filter };
  if (target.kind !== "row") return null;
  await get().loadStructure(sessionId, database, target.table);
  const primaryKey = cachedPrimaryKey(get(), sessionId, database, target.table);
  if (primaryKey?.length !== 1) {
    get().toast("error", `${target.table} has no single-column primary key, so ${formatAddress(address)} names no row`);
    return null;
  }
  return { database, table: target.table, where: [{ column: primaryKey[0], value: target.key }] };
}

/** Table tab `tab` showing `location`, with `nav` as its history. */
function retargeted(s: Store, tab: TableTab, location: TableLocation, nav: TabNav): TableTab {
  return {
    ...tab,
    ...location,
    filter: undefined,
    title: tableTitle(location.table, location.where),
    address: tableAddress(s, tab.sessionId, location),
    nav,
  };
}

function withTab(s: Store, tab: Tab): Partial<Store> {
  return { tabs: s.tabs.map((t) => (t.id === tab.id ? tab : t)) };
}

export const navSlice: Slice<NavSlice> = {
  create: (set, get) => {
    /** Moves the active table tab one step through its history. */
    const travel = async (direction: "back" | "forward") => {
      const s = get();
      const tab = selectActiveTab(s);
      if (tab?.kind !== "table") return;
      const stack = tab.nav[direction];
      const target = stack.at(-1);
      if (!target) return;
      const location = await locationOf(get, tab.sessionId, target);
      // The tab may have closed or moved on while the structure loaded.
      const current = get().tabs.find((t) => t.id === tab.id);
      if (!location || current !== tab) return;
      // The address being left goes onto the other stack.
      const leave = (entries: Address[]) => [...entries, tab.address].slice(-NAV_LIMIT);
      const nav: TabNav =
        direction === "back"
          ? { back: tab.nav.back.slice(0, -1), forward: leave(tab.nav.forward) }
          : { back: leave(tab.nav.back), forward: tab.nav.forward.slice(0, -1) };
      set((st) => withTab(st, retargeted(st, tab, location, nav)));
    };

    return {
      async openAddress(text) {
        try {
          const address = parseAddress(text.trim());
          const session = get().sessions.find((x) => x.slug === address.connection);
          if (!session) return get().toast("error", `No open connection is named ${address.connection}`);
          const kind = address.target.kind;
          if (kind !== "table" && kind !== "row") return get().toast("info", `${formatAddress(address)} cannot be opened yet: only table and row addresses open so far`);
          if (get().activeSessionId !== session.id) get().setActiveSession(session.id);
          const location = await locationOf(get, session.id, address);
          if (!location) return;
          get().setMode("db");
          get().openTableTab(location.database, location.table, "data", undefined, { where: location.where });
        } catch (e) {
          if (e instanceof AddressError) get().toast("error", e.message);
          else fail(e, "Could not open the address");
        }
      },

      showRows(tabId, location) {
        set((s) => {
          const tab = s.tabs.find((t): t is TableTab => t.id === tabId && t.kind === "table");
          if (!tab) return {};
          const next = retargeted(s, tab, location, tab.nav);
          return withTab(s, { ...next, nav: pushed(tab.nav, tab.address, next.address) });
        });
      },

      setTableFilter(tabId, filter) {
        set((s) => {
          const tab = s.tabs.find((t): t is TableTab => t.id === tabId && t.kind === "table");
          if (!tab) return {};
          return withTab(s, { ...tab, filter: filter || undefined, title: tableTitle(tab.table, tab.where, filter) });
        });
      },

      async navBack() {
        if (isWpView(get())) return get().goBack();
        await travel("back");
      },

      async navForward() {
        if (isWpView(get())) return get().goForward();
        await travel("forward");
      },

      async renameConnectionSlug(identity, slug) {
        await api.renameConnectionSlug(identity, slug);
        const moved = <A extends Address | null>(address: A): A => (address && { ...address, connection: slug }) as A;
        const movedNav = (nav: TabNav): TabNav => ({ back: nav.back.map(moved), forward: nav.forward.map(moved) });
        set((s) => {
          const ids = new Set(s.sessions.filter((x) => x.identity === identity).map((x) => x.id));
          return {
            sessions: s.sessions.map((x) => (ids.has(x.id) ? { ...x, slug } : x)),
            tabs: s.tabs.map((t): Tab => (ids.has(t.sessionId) ? ({ ...t, address: moved(t.address), nav: movedNav(t.nav) } as Tab) : t)),
          };
        });
      },
    };
  },
};
