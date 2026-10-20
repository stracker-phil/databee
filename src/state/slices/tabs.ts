import type { Home, SavedOrigin, Address, AppError, Cell, ColumnEquals, ColumnMeta, ExecuteResponse, Value } from "../../api";
import type { CellSource } from "../../lib/cells";
import type { ColumnStack, StackColumn, ValueColumn } from "../../lib/columns";
import type { ProjectForm } from "../../lib/ruleForms";
import { selectActiveSession } from "../selectors";
import { tabKind } from "../tabKinds";
import type { Slice, Store } from "../store";
import { blockedWhileLost } from "./health";
import { EMPTY_NAV, homeAddress, navEntry, pushed, tableAddress, tableTitle, type TabNav } from "./nav";
import { nextActiveSessionId } from "./session";
import { remembered } from "./ui";

export interface SavedRef {
  id: string;
  /** Preset queries are read-only. */
  origin: SavedOrigin;
  database: string;
  pin: Home | null;
}

export interface QueryTab {
  id: string;
  kind: "query";
  sessionId: string;
  title: string;
  sql: string;
  running: boolean;
  response?: ExecuteResponse;
  error?: AppError;
  activeResult: number;
  /** The database home; null while the session has no current database. */
  address: Address | null;
  nav: TabNav;
  /** A value opened to the right of the results (Enter on serialized data or long text). */
  valueColumn?: ValueColumn;
  saved?: SavedRef;
}

export interface TableTab {
  id: string;
  kind: "table";
  sessionId: string;
  title: string;
  database: string;
  table: string;
  view: "data" | "structure";
  /** Initial free-text WHERE filter, as typed. */
  filter?: string;
  /** Exact column matches Rust turns into SQL, e.g. when opened from a document's source link. */
  where: ColumnEquals[];
  /** Preview tabs (single click in the sidebar) are replaced by the next preview; pinning keeps them. */
  preview?: boolean;
  /** Derived from `table` and `where` (never from `filter`), see `tableAddress`. */
  address: Address;
  nav: TabNav;
  /** A value opened to the right of the rows (Enter on serialized data or long text). */
  valueColumn?: ValueColumn;
}

/** Rows as objects: a stack of columns, the row the tab opened first, then each drilled reference or opened value. */
export interface ObjectTab {
  id: string;
  kind: "object";
  sessionId: string;
  title: string;
  columns: StackColumn[];
  /** The column `h`/`l` move, the path marks and history records. */
  active: number;
  /** The active column's row address (a value column's: the row it came from). */
  address: Address;
  /** The stacks of the addresses in `nav`, so back and forward restore the columns too; never stored. */
  stacks: Record<string, ColumnStack>;
  /** The shares of the shown panes after a divider moved (summing to 1); never stored. */
  paneWidths?: number[];
  /** Only the active column shows, at the full width; never stored. */
  maximized?: boolean;
  preview?: boolean;
  nav: TabNav;
}

/**
 * The Project page of one database (`?project`); `form` is the rule form opened from a value or a row, `section`
 * the shown part (default: the overview).
 */
export interface ProjectTab {
  id: string;
  kind: "project";
  sessionId: string;
  title: string;
  database: string;
  address: Address;
  form: ProjectForm | null;
  section?: ProjectSection;
  nav: TabNav;
}

/** The readout of one database: its home address (`databee://<slug>/<db>/`); one per session. */
export interface ReadoutTab {
  id: string;
  kind: "readout";
  sessionId: string;
  title: string;
  database: string;
  address: Address;
  nav: TabNav;
}

export type ProjectSection = "overview" | "rules";

export type Tab = QueryTab | TableTab | ObjectTab | ProjectTab | ReadoutTab;

/** A tab that shows data in place: a table, or a row as an object; `navigateTab` turns one into the other. */
export type DataTab = TableTab | ObjectTab;

export type TabOfKind<K extends Tab["kind"]> = Extract<Tab, { kind: K }>;

/** Tab `tabId`, when it is of `kind` (any kind when absent). */
export function findTab<K extends Tab["kind"] = Tab["kind"]>(s: Store, tabId: string, kind?: K): TabOfKind<K> | undefined {
  return s.tabs.find((t): t is TabOfKind<K> => t.id === tabId && (kind === undefined || t.kind === kind));
}

/** The update that puts `tab` in the place of the tab with its id. */
export function replaceTab(s: Store, tab: Tab): Pick<Store, "tabs"> {
  return { tabs: s.tabs.map((t) => (t.id === tab.id ? tab : t)) };
}

/** A table or query tab with a value column open next to its grid. */
export const hasValueColumn = (tab: Tab): tab is TabOfKind<"table" | "query"> & { valueColumn: ValueColumn } => "valueColumn" in tab && !!tab.valueColumn;

/** A tab in its session's preview place, which the next preview replaces. */
export const isPreviewTab = (tab: Tab): tab is TableTab | ObjectTab => tabKind(tab).preview && !!(tab as TableTab | ObjectTab).preview;

/** The preview tab of a session (table or object), which the next preview replaces. */
export function previewTab(s: Store, sessionId: string): TableTab | ObjectTab | undefined {
  return s.tabs.find((t): t is TableTab | ObjectTab => isPreviewTab(t) && t.sessionId === sessionId);
}

/** `tabs` with `tab` in the place of `replaced` (or appended), so a preview keeps its position. */
export function placed(tabs: Tab[], tab: Tab, replaced: Tab | undefined): Tab[] {
  const kept = tabs.filter((t) => t.id !== replaced?.id);
  const at = replaced ? tabs.indexOf(replaced) : kept.length;
  return [...kept.slice(0, at), tab, ...kept.slice(at)];
}

let nextTabId = 1;

export const newTabId = (kind: Tab["kind"]) => `${kind}-${nextTabId++}`;

export interface SelectedCell {
  /** The tab whose grid holds the cell; closing, leaving or reloading it clears the selection. */
  tabId: string;
  row: number;
  col: number;
  column: ColumnMeta;
  cell: Cell;
  /** Lets a value column and the value commands fetch a `longText` cell in full. */
  source: CellSource;
  /** The cell as a value (kind and provenance) for the status bar and the value commands; grids always set it. */
  value?: Value;
}

export interface TabsSlice {
  tabs: Tab[];
  activeTabId: string | null;
  selectedCell: SelectedCell | null;
  /** Query tabs opened per session, for unique default titles. */
  queryCounter: Record<string, number>;
  openQueryTab: (sql?: string, title?: string, saved?: SavedRef) => void;
  /** An open tab of the same table is activated; `view` and `preview: false` (pin) apply to it only when given. */
  openTableTab: (database: string, table: string, view?: TableTab["view"], filter?: string, opts?: { preview?: boolean; where?: ColumnEquals[] }) => void;
  pinTab: (id: string) => void;
  activateTab: (id: string) => void;
  closeTab: (id: string) => void;
  cycleTab: (delta: number) => void;
  updateQueryTab: (id: string, patch: Partial<QueryTab>) => void;
  updateTableTab: (id: string, patch: Partial<TableTab>) => void;
  updateObjectTab: (id: string, patch: Partial<ObjectTab>) => void;
  selectCell: (selected: Omit<SelectedCell, "tabId"> | null) => void;
}

export const resetSelection = () => ({ selectedCell: null });

function selectionOutside(s: Store, tabId: string | null) {
  return s.selectedCell?.tabId === tabId ? null : s.selectedCell;
}

/** Activates `tabId`; the selection belongs to the tab being left. */
export function activate(s: Store, tabId: string | null) {
  return { activeTabId: tabId, selectedCell: tabId === s.activeTabId ? s.selectedCell : selectionOutside(s, s.activeTabId) };
}

function sameWhere(a: ColumnEquals[], b: ColumnEquals[]): boolean {
  return a.length === b.length && a.every((x, i) => x.column === b[i].column && x.value === b[i].value);
}

export const tabsSlice: Slice<TabsSlice> = {
  create: (set, get) => {
    const patchTab =
      <K extends Tab["kind"]>(kind: K) =>
      (id: string, patch: Partial<TabOfKind<K>>) =>
        set((s) => {
          const tab = findTab(s, id, kind);
          return tab ? replaceTab(s, { ...tab, ...patch }) : {};
        });

    return {
      tabs: [],
      activeTabId: null,
      queryCounter: {},
      ...resetSelection(),

      openQueryTab(sql = "", title, saved) {
        const session = selectActiveSession(get());
        if (!session) return;
        set((s) => {
          // Only untitled tabs take a number.
          const previous = s.queryCounter[session.id] ?? 0;
          const count = title ? previous : previous + 1;
          const tab: QueryTab = {
            id: newTabId("query"),
            kind: "query",
            sessionId: session.id,
            title: title ?? `Query ${count}`,
            sql,
            running: false,
            activeResult: 0,
            address: homeAddress(s, session.id, session.database),
            nav: EMPTY_NAV,
            saved,
          };
          return { tabs: [...s.tabs, tab], queryCounter: { ...s.queryCounter, [session.id]: count }, ...activate(s, tab.id) };
        });
      },

      openTableTab(database, table, view, filter, opts = {}) {
        const session = selectActiveSession(get());
        if (!session) return;
        const where = opts.where ?? [];
        const existing = get().tabs.find(
          (t): t is TableTab =>
            t.kind === "table" &&
            t.sessionId === session.id &&
            t.database === database &&
            t.table === table &&
            t.filter === filter &&
            sameWhere(t.where, where),
        );
        // An open tab shows the rows it loaded; a new one needs the server.
        if (!existing && blockedWhileLost(get, session.id, `open ${table}`)) return;
        set((s) => remembered(s, { kind: "table", database, table }));
        if (existing) {
          const updated: TableTab = { ...existing, view: view ?? existing.view, preview: existing.preview && opts.preview !== false };
          set((s) => ({ ...replaceTab(s, updated), ...activate(s, existing.id) }));
          return;
        }
        set((s) => {
          const address = tableAddress(s, session.id, { database, table, where });
          // A preview tab takes the place of the previous preview tab of this session, history included.
          const replaced = opts.preview ? previewTab(s, session.id) : undefined;
          const tab: TableTab = {
            id: newTabId("table"),
            kind: "table",
            sessionId: session.id,
            title: tableTitle(table, where, filter),
            database,
            table,
            view: view ?? "data",
            filter,
            where,
            preview: opts.preview ?? false,
            address,
            nav: replaced ? pushed(replaced.nav, navEntry(replaced, "table"), { kind: "table", address }) : EMPTY_NAV,
          };
          return { tabs: placed(s.tabs, tab, replaced), ...activate(s, tab.id) };
        });
      },

      pinTab(id) {
        set((s) => {
          const tab = findTab(s, id);
          return tab && isPreviewTab(tab) ? replaceTab(s, { ...tab, preview: false }) : {};
        });
      },

      activateTab(id) {
        set((s) => activate(s, id));
      },

      closeTab(id) {
        set((s) => {
          const closing = findTab(s, id);
          if (!closing) return {};
          const tabs = s.tabs.filter((t) => t.id !== id);
          const selectedCell = selectionOutside(s, id);
          if (s.activeTabId !== id) return { tabs, selectedCell };
          const sessionTabs = s.tabs.filter((t) => t.sessionId === closing.sessionId);
          const index = sessionTabs.indexOf(closing);
          const siblings = sessionTabs.filter((t) => t !== closing);
          const neighbour = siblings[Math.min(index, siblings.length - 1)];
          return { tabs, activeTabId: neighbour?.id ?? null, selectedCell };
        });
      },

      cycleTab(delta) {
        const s = get();
        const tabs = s.tabs.filter((t) => t.sessionId === s.activeSessionId);
        if (tabs.length < 2) return;
        const index = tabs.findIndex((t) => t.id === s.activeTabId);
        const next = tabs[(index + delta + tabs.length) % tabs.length];
        set(activate(s, next.id));
      },

      updateQueryTab: patchTab("query"),
      updateTableTab: patchTab("table"),
      updateObjectTab: patchTab("object"),

      selectCell(selected) {
        set((s) => ({ selectedCell: selected && s.activeTabId ? { ...selected, tabId: s.activeTabId } : null }));
      },
    };
  },

  sessionClosed(sessionId, s) {
    const tabs = s.tabs.filter((t) => t.sessionId !== sessionId);
    const activeSessionId = nextActiveSessionId(s, sessionId);
    const activeTabId = tabs.some((t) => t.id === s.activeTabId)
      ? s.activeTabId
      : (tabs.find((t) => t.sessionId === activeSessionId)?.id ?? null);
    const selectionKept = tabs.some((t) => t.id === s.selectedCell?.tabId);
    const { [sessionId]: _counter, ...queryCounter } = s.queryCounter;
    return { tabs, activeTabId, queryCounter, selectedCell: selectionKept ? s.selectedCell : null };
  },
};
