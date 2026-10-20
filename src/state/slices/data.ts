import { api } from "../../api";
import { fail } from "../requests";
import { selectActiveSession } from "../selectors";
import type { Slice } from "../store";
import { blockedWhileLost } from "./health";
import { resetSelection } from "./tabs";

export interface DataSlice {
  /** Bumped when data may have changed (applied edits, live refresh); views reload on change. */
  dataVersion: number;
  /** Bumped when a query finishes, so the history panel reloads. */
  historyVersion: number;
  bumpData: () => void;
  /** `view.refresh`: forgets the active session's cached schema, then reloads every view. */
  refreshView: () => Promise<void>;
}

export const dataSlice: Slice<DataSlice> = {
  create: (set, get) => ({
    dataVersion: 0,
    historyVersion: 0,

    bumpData() {
      // Every view reloads, so no selected cell is current anymore.
      set((s) => ({ dataVersion: s.dataVersion + 1, ...resetSelection() }));
    },

    async refreshView() {
      // The schema cache only notices DDL run in this app; a refresh also shows DDL made elsewhere.
      const session = selectActiveSession(get());
      if (blockedWhileLost(get, session?.id, "refresh")) return;
      if (session) {
        try {
          await api.refreshSchema(session.id);
        } catch (e) {
          fail(e, "Could not refresh the schema");
        }
      }
      get().bumpData();
    },
  }),
};
