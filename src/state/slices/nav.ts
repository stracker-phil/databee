/**
 * Addresses of tabs and their back/forward history. A table tab's address is derived from its table and
 * structured filter; a free SQL filter is not addressable and leaves the address as it is.
 */
import { api, type Address, type ColumnEquals } from "../../api";
import { AddressError, formatAddress, parseAddress, sameAddress } from "../../lib/address";
import { fail } from "../requests";
import { selectActiveTab, selectStructure } from "../selectors";
import type { Slice, Store } from "../store";
import { objectColumnAt, type ColumnStack, type StackColumn } from "../../lib/columns";
import { documentText, objectAddress, objectTitle } from "../../lib/document";
import type { ObjectTab, Tab, TableTab } from "./tabs";

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
  /** Opens an address given as text (a row as an object tab); errors become a toast. `preview` opens it in the preview tab. */
  openAddress(text: string, opts?: { preview?: boolean }): Promise<void>;
  /** Shows other rows in table tab `tabId`, recording the current address for back. */
  showRows(tabId: string, location: TableLocation): void;
  /** Narrows table tab `tabId` to rows where `match.column` equals `match.value`, replacing a match on the same column. */
  filterRows(tabId: string, match: ColumnEquals): void;
  /** Applies a typed WHERE filter to table tab `tabId`; not addressable, so no history entry. */
  setTableFilter(tabId: string, filter: string): void;
  /** Back and forward in the active tab. */
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

/** `tab`'s history after one step in `direction`: the address being left goes onto the other stack. */
function stepped(tab: Tab & { address: Address }, direction: "back" | "forward"): TabNav {
  const leave = (entries: Address[]) => [...entries, tab.address].slice(-NAV_LIMIT);
  return direction === "back"
    ? { back: tab.nav.back.slice(0, -1), forward: leave(tab.nav.forward) }
    : { back: leave(tab.nav.back), forward: tab.nav.forward.slice(0, -1) };
}

/** The stacks of the addresses `nav` still holds, after leaving `tab` (whose stack is kept under its address). */
function keptStacks(tab: ObjectTab, nav: TabNav): Record<string, ColumnStack> {
  const stacks = { ...tab.stacks, [formatAddress(tab.address)]: { columns: tab.columns, active: tab.active } };
  const held = new Set([...nav.back, ...nav.forward].map(formatAddress));
  return Object.fromEntries(Object.entries(stacks).filter(([key]) => held.has(key)));
}

/** Object tab `tab` showing `stack`; leaving its address records it, with its stack, for back. */
export function restacked(tab: ObjectTab, stack: ColumnStack, { record = true } = {}): ObjectTab {
  const address = objectColumnAt(stack.columns, stack.active)?.address ?? tab.address;
  const next: ObjectTab = { ...tab, ...stack, address };
  if (!record || sameAddress(tab.address, address)) return next;
  const nav = pushed(tab.nav, tab.address, address);
  return { ...next, nav, stacks: keptStacks(tab, nav) };
}

/** Without a recorded stack, the active column moves to `target`; its focus stays on the same row. */
function movedActive(tab: ObjectTab, target: Address): ColumnStack {
  const view = target.view === "rows" ? "rows" : "document";
  const current = tab.columns[tab.active];
  const focus = current?.kind === "object" && documentText(current.address) === documentText(target) ? current.focus : null;
  const column: StackColumn = { kind: "object", address: objectAddress(target, view), view, focus };
  return { columns: [...tab.columns.slice(0, tab.active), column], active: tab.active };
}

/** Object tab `tab` showing the row address `target` with the stack it had there. */
function reobjected(tab: ObjectTab, target: Address, nav: TabNav): ObjectTab {
  const stack = tab.stacks[formatAddress(target)] ?? movedActive(tab, target);
  const root = stack.columns[0];
  const oldRoot = tab.columns[0];
  const sameRoot = root?.kind === "object" && oldRoot?.kind === "object" && documentText(root.address) === documentText(oldRoot.address);
  const title = sameRoot || root?.kind !== "object" ? tab.title : objectTitle(root.address);
  const address = objectColumnAt(stack.columns, stack.active)?.address ?? target;
  return { ...tab, ...stack, address, title, nav, stacks: keptStacks(tab, nav) };
}

/** A tab of a renamed connection; recorded stacks are dropped rather than rewritten. */
function renamed(tab: Tab, slug: string): Tab {
  const moved = <A extends Address | null>(address: A): A => (address && { ...address, connection: slug }) as A;
  const nav = { back: tab.nav.back.map(moved), forward: tab.nav.forward.map(moved) };
  if (tab.kind !== "object") return { ...tab, address: moved(tab.address), nav } as Tab;
  const columns = tab.columns.map((c): StackColumn => (c.kind === "object" ? { ...c, address: moved(c.address) } : c));
  return { ...tab, columns, address: moved(tab.address), nav, stacks: {} };
}

export const navSlice: Slice<NavSlice> = {
  create: (set, get) => {
    /** Moves the active table or object tab one step through its history. */
    const travel = async (direction: "back" | "forward") => {
      const tab = selectActiveTab(get());
      const target = tab?.nav[direction].at(-1);
      if (!tab || !target || tab.kind === "query" || tab.kind === "project" || tab.kind === "readout") return;
      if (tab.kind === "object") {
        if (target.target.kind === "row") set((st) => ({ ...withTab(st, reobjected(tab, target, stepped(tab, direction))), selectedCell: null }));
        return;
      }
      const location = await locationOf(get, tab.sessionId, target);
      // The tab may have closed or moved on while the structure loaded.
      const current = get().tabs.find((t) => t.id === tab.id);
      if (!location || current !== tab) return;
      set((st) => withTab(st, retargeted(st, tab, location, stepped(tab, direction))));
    };

    return {
      async openAddress(text, opts = {}) {
        try {
          const address = parseAddress(text.trim());
          const session = get().sessions.find((x) => x.slug === address.connection);
          if (!session) return get().toast("error", `No open connection is named ${address.connection}`);
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
          const tab = s.tabs.find((t): t is TableTab => t.id === tabId && t.kind === "table");
          if (!tab) return {};
          const next = retargeted(s, tab, location, tab.nav);
          return withTab(s, { ...next, nav: pushed(tab.nav, tab.address, next.address) });
        });
      },

      filterRows(tabId, match) {
        const tab = get().tabs.find((t): t is TableTab => t.id === tabId && t.kind === "table");
        if (!tab) return;
        const same = (c: ColumnEquals) => c.column.toLowerCase() === match.column.toLowerCase();
        get().showRows(tabId, { database: tab.database, table: tab.table, where: [...tab.where.filter((c) => !same(c)), match] });
      },

      setTableFilter(tabId, filter) {
        set((s) => {
          const tab = s.tabs.find((t): t is TableTab => t.id === tabId && t.kind === "table");
          if (!tab) return {};
          return withTab(s, { ...tab, filter: filter || undefined, title: tableTitle(tab.table, tab.where, filter) });
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
      },
    };
  },
};
