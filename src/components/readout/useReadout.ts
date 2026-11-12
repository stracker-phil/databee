import { useEffect } from "react";
import { readoutKey } from "../../state/slices/readout";
import { useStore, type ReadoutTab } from "../../state/store";

/** Loads once per `dataVersion`, and again whenever the tab is shown, since recent objects change without one. */
export function useReadout(tab: ReadoutTab) {
  const entry = useStore((s) => s.readouts[readoutKey(tab.sessionId, tab.database)]);
  const dataVersion = useStore((s) => s.dataVersion);
  const shown = useStore((s) => s.activeTabId === tab.id);
  useEffect(() => {
    if (shown) void useStore.getState().loadReadout(tab.sessionId, tab.database, true);
  }, [tab.sessionId, tab.database, shown]);
  // `dataVersion` is only a refetch trigger: the effect reads nothing from it.
  useEffect(() => {
    void useStore.getState().loadReadout(tab.sessionId, tab.database);
  }, [tab.sessionId, tab.database, dataVersion]);
  return entry;
}
