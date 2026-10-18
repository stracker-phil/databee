/**
 * Addresses of tabs and their back/forward history. A table tab's address is derived from its table and
 * structured filter; a free SQL filter is not addressable and leaves the address as it is.
 */
import { api, type Address, type ColumnEquals } from "../../api";
import { AddressError, formatAddress, parseAddress, sameAddress } from "../../lib/address";
import { fail } from "../requests";
import { noConnectionNamed, selectActiveTab, selectSessionBySlug, selectStructure } from "../selectors";
import type { Slice, Store } from "../store";
import { objectColumn, objectColumnAt, type ColumnStack, type StackColumn } from "../../lib/columns";
import { documentText, objectTitle } from "../../lib/document";
import { findTab, replaceTab, type DataTab, type ObjectTab, type Tab, type TableTab } from "./tabs";

const NAV_LIMIT = 50;

/** One place in a tab's history: its address and whether a table or an object tab showed it. */
export interface NavEntry {
  kind: DataTab["kind"];
  address: Address;
  /** The column stack of an object tab that became a table tab, which keeps no `stacks`. */
  stack?: ColumnStack;
}

export interface TabNav {
  back: NavEntry[];
  forward: NavEntry[];
}

export const EMPTY_NAV: TabNav = { back: [], forward: [] };

type Direction = keyof TabNav;

/** What a table tab shows, as far as an address can say it. */
interface TableLocation {
  database: string;
  table: string;
  where: ColumnEquals[];
}

export interface NavSlice {
  /** Opens an address given as text (a row as an object tab); errors become a toast. `preview` opens it in the preview tab. */
  openAddress: (text: string, opts?: { preview?: boolean }) => Promise<void>;
  /** Shows other rows in table tab `tabId`, recording the current address for back. */
  showRows: (tabId: string, location: TableLocation) => void;
  /**
   * Moves table or object tab `tabId` in place to `address`, recording the current location for back: a row
   * makes it an object tab of that row alone, a table a table tab of it. Other addresses and tabs are ignored.
   */
  navigateTab: (tabId: string, address: Address) => void;
  /** Narrows table tab `tabId` to rows where `match.column` equals `match.value`, replacing a match on the same column. */
  filterRows: (tabId: string, match: ColumnEquals) => void;
  /** Applies a typed WHERE filter to table tab `tabId`; not addressable, so no history entry. */
  setTableFilter: (tabId: string, filter: string) => void;
  navBack: () => Promise<void>;
  navForward: () => Promise<void>;
  /** Stores a renamed connection slug and rewrites the addresses of that connection's tabs. */
  renameConnectionSlug: (identity: string, slug: string) => Promise<void>;
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

/** The whole table of a row address, unfiltered; other addresses as they are. */
export function wholeTable(address: Address): Address {
  const { target } = address;
  return target.kind === "row" ? { ...address, target: { kind: "table", table: target.table, filter: [] }, view: null, compare: null } : address;
}

/** The database home, the address of a query tab; none without a current database. */
export function homeAddress(s: Store, sessionId: string, database: string | null): Address | null {
  return database ? { connection: slugOf(s, sessionId), database, target: { kind: "home" }, view: null, compare: null } : null;
}

/** The history entry of `tab`, left for a tab of kind `next`; an object tab becoming a table keeps its stack in it. */
export function navEntry(tab: DataTab, next: DataTab["kind"] = tab.kind): NavEntry {
  const entry: NavEntry = { kind: tab.kind, address: tab.address };
  return tab.kind === "object" && next === "table" ? { ...entry, stack: { columns: tab.columns, active: tab.active } } : entry;
}

const sameEntry = (a: NavEntry, b: NavEntry) => a.kind === b.kind && sameAddress(a.address, b.address);

/** `nav` after leaving `from`; leaving an entry for itself records nothing. */
export function pushed(nav: TabNav, from: NavEntry, to: NavEntry): TabNav {
  if (sameEntry(from, to)) return nav;
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

/** `tab` as a table tab of `location`; an object tab becomes one in the data view. */
function retargeted(s: Store, tab: DataTab, location: TableLocation, nav: TabNav): TableTab {
  const kept = tab.kind === "table" ? tab : { id: tab.id, sessionId: tab.sessionId, preview: tab.preview, view: "data" as const };
  return {
    ...kept,
    kind: "table",
    ...location,
    filter: undefined,
    title: tableTitle(location.table, location.where),
    address: tableAddress(s, tab.sessionId, location),
    nav,
  };
}

/** `tab`'s history after one step in `direction` to a tab of kind `next`: the entry being left goes onto the other stack. */
function stepped(tab: DataTab, direction: Direction, next: DataTab["kind"]): TabNav {
  const leave = (entries: NavEntry[]) => [...entries, navEntry(tab, next)].slice(-NAV_LIMIT);
  return direction === "back"
    ? { back: tab.nav.back.slice(0, -1), forward: leave(tab.nav.forward) }
    : { back: leave(tab.nav.back), forward: tab.nav.forward.slice(0, -1) };
}

/** The stacks of the addresses `nav` still holds, after leaving `tab` (whose stack is kept under its address). */
function keptStacks(tab: ObjectTab, nav: TabNav): Record<string, ColumnStack> {
  const stacks = { ...tab.stacks, [formatAddress(tab.address)]: { columns: tab.columns, active: tab.active } };
  const held = new Set([...nav.back, ...nav.forward].filter((e) => e.kind === "object").map((e) => formatAddress(e.address)));
  return Object.fromEntries(Object.entries(stacks).filter(([key]) => held.has(key)));
}

/** Object tab `tab` showing `stack`; leaving its address records it, with its stack, for back. */
export function restacked(tab: ObjectTab, stack: ColumnStack, { record = true } = {}): ObjectTab {
  const address = objectColumnAt(stack.columns, stack.active)?.address ?? tab.address;
  const next: ObjectTab = { ...tab, ...stack, address };
  if (!record || sameAddress(tab.address, address)) return next;
  const nav = pushed(tab.nav, navEntry(tab), { kind: "object", address });
  return { ...next, nav, stacks: keptStacks(tab, nav) };
}

/** Without a recorded stack, the active column moves to `target`; its focus stays on the same row. */
function movedActive(tab: ObjectTab, target: Address): ColumnStack {
  const current = tab.columns[tab.active];
  const focus = current?.kind === "object" && documentText(current.address) === documentText(target) ? current.focus : null;
  const column: StackColumn = { ...objectColumn(target), focus };
  return { columns: [...tab.columns.slice(0, tab.active), column], active: tab.active };
}

/** `tab` as an object tab showing `stack`; a table tab becomes one, keeping its id, preview and history, an object tab its pane layout. */
function objectified(tab: DataTab, stack: ColumnStack, nav: TabNav): ObjectTab {
  const root = stack.columns[0];
  const oldRoot = tab.kind === "object" ? tab.columns[0] : undefined;
  const sameRoot = root?.kind === "object" && oldRoot?.kind === "object" && documentText(root.address) === documentText(oldRoot.address);
  const title = sameRoot || root?.kind !== "object" ? tab.title : objectTitle(root.address);
  const address = objectColumnAt(stack.columns, stack.active)?.address ?? tab.address;
  const kept =
    tab.kind === "object"
      ? { stacks: keptStacks(tab, nav), paneWidths: tab.paneWidths, maximized: tab.maximized && stack.columns.length > 1 ? true : undefined }
      : { stacks: {} };
  return { id: tab.id, kind: "object", sessionId: tab.sessionId, title, ...stack, address, ...kept, preview: tab.preview, nav };
}

/** The stack an object entry had: recorded in the tab or the entry, else the active column moved to it (a table tab: the row alone). */
function entryStack(tab: DataTab, target: NavEntry): ColumnStack {
  if (tab.kind === "table") return target.stack ?? { columns: [objectColumn(target.address)], active: 0 };
  return tab.stacks[formatAddress(target.address)] ?? target.stack ?? movedActive(tab, target.address);
}

/** The update that moves tab `next` in place; its grid selection belongs to the view it left. */
function inPlace(s: Store, next: DataTab): Partial<Store> {
  return { ...replaceTab(s, next), selectedCell: s.selectedCell?.tabId === next.id ? null : s.selectedCell };
}

/** A tab of a renamed connection; recorded stacks are dropped rather than rewritten. */
function renamed(tab: Tab, slug: string): Tab {
  const moved = <A extends Address | null>(address: A): A => (address && { ...address, connection: slug });
  const movedEntry = ({ kind, address }: NavEntry): NavEntry => ({ kind, address: moved(address) });
  const nav = { back: tab.nav.back.map(movedEntry), forward: tab.nav.forward.map(movedEntry) };
  if (tab.kind !== "object") return { ...tab, address: moved(tab.address), nav } as Tab;
  const columns = tab.columns.map((c): StackColumn => (c.kind === "object" ? { ...c, address: moved(c.address) } : c));
  return { ...tab, columns, address: moved(tab.address), nav, stacks: {} };
}

export const navSlice: Slice<NavSlice> = {
  create: (set, get) => {
    const travel = async (direction: Direction) => {
      const tab = selectActiveTab(get());
      const target = tab?.nav[direction].at(-1);
      if (!tab || !target || (tab.kind !== "table" && tab.kind !== "object")) return;
      const nav = stepped(tab, direction, target.kind);
      if (target.kind === "object") {
        if (target.address.target.kind === "row") set((st) => inPlace(st, objectified(tab, entryStack(tab, target), nav)));
        return;
      }
      const location = await locationOf(get, tab.sessionId, target.address);
      // The tab may have closed or moved on while the structure loaded.
      if (!location || findTab(get(), tab.id) !== tab) return;
      set((st) => inPlace(st, retargeted(st, tab, location, nav)));
    };

    return {
      async openAddress(text, opts = {}) {
        try {
          const address = parseAddress(text.trim());
          const session = selectSessionBySlug(get(), address.connection);
          if (!session) return get().toast("error", noConnectionNamed(address.connection));
          const kind = address.target.kind;
          if (kind === "home") return get().openReadout({ sessionId: session.id, database: address.database });
          if (kind === "project") return get().openProject({ sessionId: session.id, database: address.database });
          if (address.target.kind === "find") return get().setPaletteOpen(true, `=${address.target.value}`);
          if (address.target.kind === "query") {
            const id = address.target.id;
            const entry = (await get().loadSavedQueries(session.id, address.database)).find((e) => e.id === id);
            if (!entry) return get().toast("error", `${formatAddress(address)} names no saved query`);
            return get().openSavedQuery(session.id, address.database, entry);
          }
          if (kind === "row") return get().openObject(address, { preview: opts.preview });
          if (get().activeSessionId !== session.id) get().setActiveSession(session.id);
          const location = await locationOf(get, session.id, address);
          if (!location) return;
          get().openTableTab(location.database, location.table, "data", undefined, { where: location.where, preview: opts.preview });
        } catch (e) {
          if (e instanceof AddressError) get().toast("error", e.message);
          else fail(e, "Could not open the address");
        }
      },

      showRows(tabId, location) {
        set((s) => {
          const tab = findTab(s, tabId, "table");
          if (!tab) return {};
          const next = retargeted(s, tab, location, tab.nav);
          return replaceTab(s, { ...next, nav: pushed(tab.nav, navEntry(tab), navEntry(next)) });
        });
      },

      navigateTab(tabId, address) {
        set((s) => {
          const tab = findTab(s, tabId);
          if (tab?.kind !== "table" && tab?.kind !== "object") return {};
          const { target } = address;
          if (target.kind === "row") {
            const columns = [objectColumn(address)];
            const nav = pushed(tab.nav, navEntry(tab, "object"), { kind: "object", address: columns[0].address });
            return inPlace(s, objectified(tab, { columns, active: 0 }, nav));
          }
          if (target.kind !== "table") return {};
          const next = retargeted(s, tab, { database: address.database, table: target.table, where: target.filter }, tab.nav);
          return inPlace(s, { ...next, nav: pushed(tab.nav, navEntry(tab, "table"), navEntry(next)) });
        });
      },

      filterRows(tabId, match) {
        const tab = findTab(get(), tabId, "table");
        if (!tab) return;
        const same = (c: ColumnEquals) => c.column.toLowerCase() === match.column.toLowerCase();
        get().showRows(tabId, { database: tab.database, table: tab.table, where: [...tab.where.filter((c) => !same(c)), match] });
      },

      setTableFilter(tabId, filter) {
        set((s) => {
          const tab = findTab(s, tabId, "table");
          if (!tab) return {};
          return replaceTab(s, { ...tab, filter: filter || undefined, title: tableTitle(tab.table, tab.where, filter) });
        });
      },

      async navBack() {
        await travel("back");
      },

      async navForward() {
        await travel("forward");
      },

      async renameConnectionSlug(identity, slug) {
        await api.renameConnectionSlug(identity, slug);
        set((s) => {
          const ids = new Set(s.sessions.filter((x) => x.identity === identity).map((x) => x.id));
          return {
            sessions: s.sessions.map((x) => (ids.has(x.id) ? { ...x, slug } : x)),
            tabs: s.tabs.map((t) => (ids.has(t.sessionId) ? renamed(t, slug) : t)),
          };
        });
        void get().loadAccents();
      },
    };
  },
};
