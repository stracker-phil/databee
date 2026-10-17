import { api, toAppError } from "../../api";
import { fail } from "../requests";
import type { Slice } from "../store";
import { findTab } from "./tabs";

export interface QuerySlice {
  runQuery: (tabId: string, sql: string, confirmed?: boolean) => Promise<void>;
  cancelQuery: (tabId: string) => Promise<void>;
}

export const querySlice: Slice<QuerySlice> = {
  create: (set, get) => ({
    async runQuery(tabId, sql, confirmed = false) {
      const tab = findTab(get(), tabId, "query");
      // `running` is set synchronously below, so a second run cannot start before the first replies.
      if (!tab || tab.running || !sql.trim()) return;
      get().updateQueryTab(tabId, { running: true, error: undefined });
      try {
        const response = await api.execute(tab.sessionId, sql, confirmed);
        get().updateQueryTab(tabId, {
          running: false,
          response,
          error: undefined,
          activeResult: Math.max(0, response.results.length - 1),
        });
        const session = get().sessions.find((s) => s.id === tab.sessionId);
        if (session && response.database !== session.database) {
          set((s) => ({
            sessions: s.sessions.map((x) => (x.id === tab.sessionId ? { ...x, database: response.database } : x)),
          }));
        }
        // Schema may have changed (CREATE/DROP/ALTER); table lists are cheap to reload.
        if (response.database && response.results.some((r) => !r.returnsRows)) {
          void get().loadTables(tab.sessionId, response.database);
        }
      } catch (e) {
        const error = toAppError(e);
        if (error.kind === "confirmation_required") {
          get().updateQueryTab(tabId, { running: false });
          set({
            dialog: {
              type: "confirm",
              title: "Run destructive statement?",
              message: error.reason ?? error.message,
              confirmLabel: "Run anyway",
              onConfirm: () => void get().runQuery(tabId, sql, true),
            },
          });
        } else {
          get().updateQueryTab(tabId, { running: false, error });
          if (error.sessionReset) {
            get().toast("info", "Session reset after timeout: temporary tables, variables and open transactions are gone");
          }
        }
      } finally {
        set((s) => ({ historyVersion: s.historyVersion + 1 }));
      }
    },

    async cancelQuery(tabId) {
      const tab = findTab(get(), tabId);
      if (!tab) return;
      try {
        await api.cancelQuery(tab.sessionId);
      } catch (e) {
        fail(e, "Cancel failed");
      }
    },
  }),
};
