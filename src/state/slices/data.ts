import type { Slice } from "../store";
import { resetSelection } from "./tabs";

export interface DataSlice {
  /** Bumped when data may have changed (applied edits, live refresh); views reload on change. */
  dataVersion: number;
  /** Bumped when a query finishes, so the history panel reloads. */
  historyVersion: number;
  bumpData(): void;
}

export const dataSlice: Slice<DataSlice> = {
  create: (set) => ({
    dataVersion: 0,
    historyVersion: 0,

    bumpData() {
      // Every view reloads, so no selected cell is current anymore.
      set((s) => ({ dataVersion: s.dataVersion + 1, ...resetSelection() }));
    },
  }),
};
