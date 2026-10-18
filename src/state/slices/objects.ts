import { api, toAppError, type Address, type AppError, type Document, type Provenance } from "../../api";
import { sameAddress } from "../../lib/address";
import { objectColumn, type ObjectColumn } from "../../lib/columns";
import { documentText, objectAddress, objectTitle, type ObjectView } from "../../lib/document";
import { fail } from "../requests";
import { noConnectionNamed, selectActiveSession, selectSessionBySlug } from "../selectors";
import type { Slice } from "../store";
import { EMPTY_NAV, navEntry, pushed, restacked } from "./nav";
import { withoutSession } from "./session";
import { activate, findTab, newTabId, placed, previewTab, replaceTab, type ObjectTab } from "./tabs";
import { remembered } from "./ui";

/** A `document` reply for one `dataVersion`; the previous document stays while reloading. */
export interface DocumentEntry {
  version: number;
  document: Document | null;
  error: AppError | null;
}

export interface ObjectsSlice {
  /** Documents by `session/<address or shorthand>`; `loadDocument` fills them. */
  documents: Record<string, DocumentEntry>;
  /** Loads the document of `text` (a row address, or `order 4711` in `database`) once per `dataVersion`. */
  loadDocument: (sessionId: string, text: string, database: string | null) => Promise<void>;
  /** Keeps a document fetched elsewhere, so the tab that shows it next does not fetch it again. */
  keepDocument: (sessionId: string, document: Document) => void;
  /** Opens a row address as an object tab; a tab that opened with the same row is reused. */
  openObject: (address: Address, opts?: { preview?: boolean }) => void;
  /** Opens an entity record (`order 4711`) as an object tab; Rust picks the table that holds the key. */
  openEntity: (entity: string, id: string) => Promise<void>;
  /**
   * Shows the document or the rows in column `index` (default: the active one), recording the address being
   * left; `focus` moves the focused value along.
   */
  setObjectView: (tabId: string, view: ObjectView, focus?: Provenance | null, index?: number) => void;
  /** The other view, focused on the same provenance (a grid's selected cell in Rows view); `object.viewDocument`/`object.viewRows` and opening a value use it. */
  toggleObjectView: (tabId: string, index?: number) => void;
  focusObject: (tabId: string, focus: Provenance | null, index?: number) => void;
}

export const documentKey = (sessionId: string, text: string) => `${sessionId}/${text}`;

const sameRow = (a: Address, b: Address) => sameAddress(objectAddress(a, "document"), objectAddress(b, "document"));

function columnOf(tab: ObjectTab, index = tab.active): { column: ObjectColumn; at: number } | null {
  const column = tab.columns[index];
  return column?.kind === "object" ? { column, at: index } : null;
}

const replaced = (tab: ObjectTab, at: number, column: ObjectColumn) => tab.columns.map((c, i) => (i === at ? column : c));

export const objectsSlice: Slice<ObjectsSlice> = {
  create: (set, get) => ({
    documents: {},

    async loadDocument(sessionId, text, database) {
      const key = documentKey(sessionId, text);
      const version = get().dataVersion;
      const cached: DocumentEntry | undefined = get().documents[key];
      if (cached?.version === version) return;
      const write = (entry: Omit<DocumentEntry, "version">) => set((s) => ({ documents: { ...s.documents, [key]: { ...entry, version } } }));
      const current = () => get().documents[key]?.version === version;
      write({ document: cached?.document ?? null, error: cached?.error ?? null });
      try {
        const document = await api.document(sessionId, text, database);
        if (current()) write({ document, error: null });
      } catch (e) {
        if (current()) write({ document: null, error: toAppError(e) });
      }
    },

    keepDocument(sessionId, document) {
      const key = documentKey(sessionId, documentText(document.address));
      set((s) => ({ documents: { ...s.documents, [key]: { version: s.dataVersion, document, error: null } } }));
    },

    openObject(address, opts = {}) {
      if (address.target.kind !== "row") return;
      const session = selectSessionBySlug(get(), address.connection);
      if (!session) return get().toast("error", noConnectionNamed(address.connection));
      if (get().activeSessionId !== session.id) get().setActiveSession(session.id);
      void api.rememberObject(session.id, documentText(address)).catch((e) => fail(e, "Could not keep the recent object"));
      const root = objectColumn(address);
      const existing = get().tabs.find((t): t is ObjectTab => t.kind === "object" && t.sessionId === session.id && t.columns[0]?.kind === "object" && sameRow(t.columns[0].address, address));
      if (existing) {
        const columns = address.view || address.compare ? replaced(existing, 0, { ...(existing.columns[0] as ObjectColumn), view: root.view, address: root.address }) : existing.columns;
        const updated = restacked({ ...existing, preview: existing.preview && opts.preview !== false }, { columns, active: 0 }, { record: false });
        set((s) => ({ ...replaceTab(s, updated), ...activate(s, existing.id) }));
        return;
      }
      set((s) => {
        const replacedTab = opts.preview ? previewTab(s, session.id) : undefined;
        const tab: ObjectTab = {
          id: newTabId("object"),
          kind: "object",
          sessionId: session.id,
          title: objectTitle(root.address),
          columns: [root],
          active: 0,
          address: root.address,
          stacks: {},
          preview: opts.preview ?? false,
          nav: replacedTab ? pushed(replacedTab.nav, navEntry(replacedTab, "object"), { kind: "object", address: root.address }) : EMPTY_NAV,
        };
        return { tabs: placed(s.tabs, tab, replacedTab), ...activate(s, tab.id) };
      });
    },

    async openEntity(entity, id) {
      const session = selectActiveSession(get());
      if (!session?.database) return;
      try {
        const document = await api.document(session.id, `${entity} ${id}`, session.database);
        get().keepDocument(session.id, document);
        get().openObject(document.address);
        set((s) => remembered(s, { kind: "entity", entity, id, title: `${entity} #${id}` }));
      } catch (e) {
        fail(e, `Could not open ${entity} #${id}`);
      }
    },

    setObjectView(tabId, view, focus, index) {
      set((s) => {
        const tab = findTab(s, tabId, "object");
        const found = tab && columnOf(tab, index);
        if (!tab || !found) return {};
        const { column, at } = found;
        if (column.view === view) return focus === undefined ? {} : replaceTab(s, { ...tab, columns: replaced(tab, at, { ...column, focus }) });
        const shown: ObjectColumn = { ...column, view, address: objectAddress(column.address, view), focus: focus === undefined ? column.focus : focus };
        const updated = restacked(tab, { columns: replaced(tab, at, shown), active: at });
        // The grids of the view being left unmount, so their selection goes too.
        return { ...replaceTab(s, updated), selectedCell: s.selectedCell?.tabId === tabId ? null : s.selectedCell };
      });
    },

    toggleObjectView(tabId, index) {
      const s = get();
      const tab = findTab(s, tabId, "object");
      const found = tab && columnOf(tab, index);
      if (!tab || !found) return;
      const { column, at } = found;
      if (column.view === "document") return s.setObjectView(tabId, "rows", undefined, at);
      const selected = s.selectedCell?.tabId === tabId && at === tab.active ? s.selectedCell.value?.source : null;
      s.setObjectView(tabId, "document", selected ?? column.focus, at);
    },

    focusObject(tabId, focus, index) {
      set((s) => {
        const tab = findTab(s, tabId, "object");
        const found = tab && columnOf(tab, index);
        if (!tab || !found) return {};
        return replaceTab(s, { ...tab, columns: replaced(tab, found.at, { ...found.column, focus }) });
      });
    },
  }),

  sessionClosed: (sessionId, s) => ({ documents: withoutSession(s.documents, sessionId) }),
};
