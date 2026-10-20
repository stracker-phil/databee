import { api, toAppError, type AppError, type Provenance, type Readout } from "../../api";
import { fail } from "../requests";
import { selectActiveSession } from "../selectors";
import type { Slice, Store } from "../store";
import { EMPTY_NAV, homeAddress } from "./nav";
import { withoutSession } from "./session";
import { activate, newTabId, replaceTab, type ReadoutTab } from "./tabs";

interface ReadoutEntry {
  version: number;
  readout: Readout | null;
  error: AppError | null;
}

export interface ReadoutSlice {
  /** By `session/database`. */
  readouts: Record<string, ReadoutEntry>;
  /** Loads once per `dataVersion`; `force` reloads anyway (the recent objects change without one). */
  loadReadout: (sessionId: string, database: string, force?: boolean) => Promise<void>;
  /**
   * Shows the readout of `database` (default: the active session's current one) in the session's readout tab,
   * which is created first when there is none. `activate: false` only moves an existing tab to the database.
   */
  openReadout: (opts?: { sessionId?: string; database?: string; activate?: boolean }) => void;
  /** Watches the cell `source` (`path` empty) or the node at `path` in its decoded value on the readout of `database`. */
  watchValue: (sessionId: string, database: string, source: Provenance, path: string[]) => Promise<void>;
  unwatchValue: (sessionId: string, database: string, id: string) => Promise<void>;
  /** `null` or a blank label goes back to the default one. */
  labelWatched: (sessionId: string, database: string, id: string, label: string | null) => Promise<void>;
}

export const readoutKey = (sessionId: string, database: string) => `${sessionId}/${database}`;

function readoutTab(s: Store, sessionId: string): ReadoutTab | undefined {
  return s.tabs.find((t): t is ReadoutTab => t.kind === "readout" && t.sessionId === sessionId);
}

/** Reads a loaded readout again after the change; a failure toasts and resolves to false. */
async function changeWatched(get: () => Store, sessionId: string, database: string, change: () => Promise<unknown>, failure: string): Promise<boolean> {
  try {
    await change();
    if (get().readouts[readoutKey(sessionId, database)]) await get().loadReadout(sessionId, database, true);
    return true;
  } catch (e) {
    fail(e, failure);
    return false;
  }
}

export const readoutSlice: Slice<ReadoutSlice> = {
  create: (set, get) => ({
    readouts: {},

    async loadReadout(sessionId, database, force = false) {
      const key = readoutKey(sessionId, database);
      const version = get().dataVersion;
      const cached: ReadoutEntry | undefined = get().readouts[key];
      if (cached?.version === version && !force) return;
      const write = (entry: Omit<ReadoutEntry, "version">) => set((s) => ({ readouts: { ...s.readouts, [key]: { ...entry, version } } }));
      const current = () => get().readouts[key]?.version === version;
      write({ readout: cached?.readout ?? null, error: null });
      try {
        const readout = await api.readout(sessionId, database);
        if (current()) write({ readout, error: null });
      } catch (e) {
        if (current()) write({ readout: null, error: toAppError(e) });
      }
    },

    openReadout(opts = {}) {
      const s = get();
      const session = opts.sessionId ? s.sessions.find((x) => x.id === opts.sessionId) : selectActiveSession(s);
      const database = opts.database ?? session?.database;
      const address = session && homeAddress(s, session.id, database ?? null);
      if (!session || !database || !address) return;
      const existing = readoutTab(s, session.id);
      const activating = opts.activate !== false;
      if (!existing && !activating) return;
      if (activating && s.activeSessionId !== session.id) s.setActiveSession(session.id);
      set((st) => {
        const tab: ReadoutTab = existing
          ? { ...existing, database, address }
          : { id: newTabId("readout"), kind: "readout", sessionId: session.id, title: "readout", database, address, nav: EMPTY_NAV };
        return { ...(existing ? replaceTab(st, tab) : { tabs: [tab, ...st.tabs] }), ...(activating ? activate(st, tab.id) : {}) };
      });
    },

    async watchValue(sessionId, database, source, path) {
      const watched = await changeWatched(get, sessionId, database, () => api.readoutWatch(sessionId, database, source, path), "Could not watch the value");
      if (watched) get().toast("success", `Watching ${source.table}.${source.column} on the readout`);
    },

    async unwatchValue(sessionId, database, id) {
      await changeWatched(get, sessionId, database, () => api.readoutUnwatch(sessionId, database, id), "Could not remove the watched value");
    },

    async labelWatched(sessionId, database, id, label) {
      await changeWatched(get, sessionId, database, () => api.readoutWatchLabel(sessionId, database, id, label), "Could not rename the watched value");
    },
  }),

  sessionClosed: (sessionId, s) => ({ readouts: withoutSession(s.readouts, sessionId) }),
};
