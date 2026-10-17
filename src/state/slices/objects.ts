/** Object tabs (a row as its document or its rows) and the documents they show. */
import { api, toAppError, type Address, type AppError, type Document, type Provenance } from "../../api";
import { sameAddress } from "../../lib/address";
import { documentText, objectAddress, objectTitle, type ObjectView } from "../../lib/document";
import type { Slice, Store } from "../store";
import { EMPTY_NAV, pushed } from "./nav";
import { withoutSession } from "./session";
import { activate, newTabId, placed, previewTab, type ObjectTab } from "./tabs";

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
  loadDocument(sessionId: string, text: string, database: string | null): Promise<void>;
  /** Keeps a document fetched elsewhere, so the tab that shows it next does not fetch it again. */
  keepDocument(sessionId: string, document: Document): void;
  /** Opens a row address as an object tab; an open tab of the same row is reused. */
  openObject(address: Address, opts?: { preview?: boolean }): void;
  /** Shows the document or the rows, recording the address being left; `focus` moves the focused value along. */
  setObjectView(tabId: string, view: ObjectView, focus?: Provenance | null): void;
  /** `object.toggleView`: the other view, focused on the same provenance (a grid's selected cell in Rows view). */
  toggleObjectView(tabId: string): void;
  focusObject(tabId: string, focus: Provenance | null): void;
}

export const documentKey = (sessionId: string, text: string) => `${sessionId}/${text}`;

const findObjectTab = (s: Store, tabId: string) => s.tabs.find((t): t is ObjectTab => t.id === tabId && t.kind === "object");

function withObjectTab(s: Store, tab: ObjectTab): Partial<Store> {
  return { tabs: s.tabs.map((t) => (t.id === tab.id ? tab : t)) };
}

const sameRow = (a: Address, b: Address) => sameAddress(objectAddress(a, "document"), objectAddress(b, "document"));

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
      const session = get().sessions.find((x) => x.slug === address.connection);
      if (!session) return get().toast("error", `No open connection is named ${address.connection}`);
      if (get().activeSessionId !== session.id) get().setActiveSession(session.id);
      get().setMode("db");
      const view: ObjectView = address.view === "rows" ? "rows" : "document";
      const shown = objectAddress(address, view);
      const existing = get().tabs.find((t): t is ObjectTab => t.kind === "object" && t.sessionId === session.id && sameRow(t.address, address));
      if (existing) {
        const viewed = address.view ? { view, address: shown } : {};
        const updated: ObjectTab = { ...existing, ...viewed, preview: existing.preview && opts.preview !== false };
        set((s) => ({ ...withObjectTab(s, updated), ...activate(s, existing.id) }));
        return;
      }
      set((s) => {
        const replaced = opts.preview ? previewTab(s, session.id) : undefined;
        const tab: ObjectTab = {
          id: newTabId("object"),
          kind: "object",
          sessionId: session.id,
          title: objectTitle(shown),
          address: shown,
          view,
          focus: null,
          preview: opts.preview ?? false,
          nav: replaced ? pushed(replaced.nav, replaced.address, shown) : EMPTY_NAV,
        };
        return { tabs: placed(s.tabs, tab, replaced), ...activate(s, tab.id) };
      });
    },

    setObjectView(tabId, view, focus) {
      set((s) => {
        const tab = findObjectTab(s, tabId);
        if (!tab || tab.view === view) return {};
        const address = objectAddress(tab.address, view);
        const updated: ObjectTab = { ...tab, view, address, focus: focus === undefined ? tab.focus : focus, nav: pushed(tab.nav, tab.address, address) };
        // The grids of the view being left unmount, so their selection goes too.
        return { ...withObjectTab(s, updated), selectedCell: s.selectedCell?.tabId === tabId ? null : s.selectedCell };
      });
    },

    toggleObjectView(tabId) {
      const s = get();
      const tab = findObjectTab(s, tabId);
      if (!tab) return;
      if (tab.view === "document") return s.setObjectView(tabId, "rows");
      const selected = s.selectedCell?.tabId === tabId ? s.selectedCell.value?.source : null;
      s.setObjectView(tabId, "document", selected ?? tab.focus);
    },

    focusObject(tabId, focus) {
      set((s) => {
        const tab = findObjectTab(s, tabId);
        return tab ? withObjectTab(s, { ...tab, focus }) : {};
      });
    },
  }),

  sessionClosed: (sessionId, s) => ({ documents: withoutSession(s.documents, sessionId) }),
};
