/** Saved queries (docs/frontend.md, Saved queries): the project's, the global ones and the presets' pinned ones. */
import { api, type Home, type SavedEntry, type SavedOrigin, type SavedQuery } from "../../api";
import { fail } from "../requests";
import { selectActiveTab } from "../selectors";
import type { Slice, Store } from "../store";
import { withoutSession } from "./session";
import { projectKey } from "./project";
import type { QueryTab, SavedRef } from "./tabs";

export interface SavedSlice {
  /** By `session/database`. */
  savedQueries: Record<string, SavedEntry[]>;
  loadSavedQueries(sessionId: string, database: string): Promise<SavedEntry[]>;
  /** A query tab with the entry's SQL, run at once; a preset query opens as an unsaved "copy of …". */
  openSavedQuery(sessionId: string, database: string, entry: SavedEntry): void;
  /** ⌘S: updates the tab's project or global query, else asks for a title and stores a new one in the project. */
  saveQueryTab(tabId: string): void;
  /** Asks how the tab's saved query shows on the readout: not at all, as a count or as its first rows. */
  pinQueryTab(tabId: string): void;
  /** Copies the preset query the tab was opened from into the project; the tab then saves to that copy. */
  copyPresetQuery(tabId: string): Promise<void>;
}

export const isPreset = (origin: SavedOrigin): origin is Extract<SavedOrigin, { kind: "preset" }> => origin.kind === "preset";

/** `project`, `global` or `preset:<name>`, as the palette and the query tab show it. */
export function originText(origin: SavedOrigin): string {
  return origin.kind === "preset" ? `preset:${origin.preset}` : origin.kind;
}

const sameOrigin = (a: SavedOrigin, b: SavedOrigin) => originText(a) === originText(b);

function queryTab(s: Store, tabId: string): QueryTab | null {
  const tab = s.tabs.find((t) => t.id === tabId);
  return tab?.kind === "query" ? tab : null;
}

/** The database a tab's query is saved in: the one it was saved to, else the session's current one. */
function tabDatabase(s: Store, tab: QueryTab): string | null {
  return tab.saved?.database ?? s.sessions.find((x) => x.id === tab.sessionId)?.database ?? null;
}

function savedRef(query: SavedQuery, origin: SavedOrigin, database: string): SavedRef {
  return { id: query.id, origin, database, pin: query.pin ?? null };
}

export const savedSlice: Slice<SavedSlice> = {
  create: (set, get) => {

    /** Runs a store request and points the tab at the stored query. */
    const persist = async (tab: QueryTab, database: string, origin: SavedOrigin, request: () => Promise<SavedQuery>, done: (title: string) => string, failure: string) => {
      try {
        const query = await request();
        get().updateQueryTab(tab.id, { title: query.title, saved: savedRef(query, origin, database) });
        get().toast("success", done(query.title));
        void get().loadSavedQueries(tab.sessionId, database);
      } catch (e) {
        fail(e, failure);
      }
    };

    const storeNew = (tab: QueryTab, database: string, title: string) =>
      persist(tab, database, { kind: "project" }, () => api.saveQuery(tab.sessionId, database, title, tab.sql, null), (t) => `Saved ${t}`, "Could not save the query");

    const storeChange = (tab: QueryTab, ref: SavedRef, pin: Home | null) =>
      persist(tab, ref.database, ref.origin, () => api.updateSavedQuery(tab.sessionId, ref.database, { id: ref.id, title: tab.title, sql: tab.sql, pin }), (t) => `Saved ${t}`, "Could not save the query");

    return {
      savedQueries: {},

      async loadSavedQueries(sessionId, database) {
        try {
          const entries = await api.savedQueries(sessionId, database);
          set((s) => ({ savedQueries: { ...s.savedQueries, [projectKey(sessionId, database)]: entries } }));
          return entries;
        } catch (e) {
          fail(e, "Could not load saved queries");
          return [];
        }
      },

      openSavedQuery(sessionId, database, entry) {
        const s = get();
        if (!entry.readOnly) {
          const showsEntry = (t: Store["tabs"][number]) => t.kind === "query" && t.sessionId === sessionId && t.saved?.id === entry.id && sameOrigin(t.saved.origin, entry.origin);
          const open = s.tabs.find(showsEntry);
          if (open) return s.activateTab(open.id);
        }
        if (s.activeSessionId !== sessionId) s.setActiveSession(sessionId);
        const title = entry.readOnly ? `copy of ${entry.title}` : entry.title;
        s.openQueryTab(entry.sql, title, { id: entry.id, origin: entry.origin, database, pin: entry.pin });
        const tab = selectActiveTab(get());
        if (tab?.kind === "query") void get().runQuery(tab.id, entry.sql);
      },

      saveQueryTab(tabId) {
        const s = get();
        const tab = queryTab(s, tabId);
        if (!tab) return;
        if (!tab.sql.trim()) return s.toast("info", "Write a query first, then save it");
        const database = tabDatabase(s, tab);
        if (!database) return s.toast("info", "Choose a database first: saved queries belong to one");
        const ref = tab.saved;
        if (ref && !isPreset(ref.origin)) return void storeChange(tab, ref, ref.pin);
        s.openDialog({ type: "prompt", title: "Save query as", value: tab.title, onSubmit: (title) => void storeNew(tab, database, title) });
      },

      pinQueryTab(tabId) {
        const s = get();
        const tab = queryTab(s, tabId);
        if (!tab) return;
        const ref = tab.saved;
        if (!ref || isPreset(ref.origin)) return s.toast("info", "Save the query first, then pin it");
        s.openDialog({
          type: "pin",
          title: tab.title,
          pin: ref.pin,
          onSubmit: (pin) => {
            const current = queryTab(get(), tabId);
            if (current) void storeChange(current, ref, pin);
          },
        });
      },

      async copyPresetQuery(tabId) {
        const tab = queryTab(get(), tabId);
        const ref = tab?.saved;
        if (!tab || !ref || !isPreset(ref.origin)) return;
        const preset = ref.origin.preset;
        await persist(tab, ref.database, { kind: "project" }, () => api.copyPresetQueryToProject(tab.sessionId, ref.database, preset, ref.id), (t) => `Copied ${t} to the project`, "Could not copy the query to the project");
      },
    };
  },

  sessionClosed(sessionId, s) {
    return { savedQueries: withoutSession(s.savedQueries, sessionId) };
  },
};
