import { useEffect } from "react";
import { selectDocument, useStore } from "../../state/store";

/**
 * The document of `text` (a row address, or an entity shorthand such as `order 4711` in `database`) from the
 * store cache: one request per `dataVersion`, so ⌘R and applied writes refresh it.
 */
export function useDocument(sessionId: string, text: string, database: string | null) {
  const entry = useStore((s) => selectDocument(s, sessionId, text));
  const dataVersion = useStore((s) => s.dataVersion);
  // `dataVersion` is only a refetch trigger: the effect reads nothing from it.
  useEffect(() => {
    void useStore.getState().loadDocument(sessionId, text, database);
  }, [sessionId, text, database, dataVersion]);
  return { document: entry?.document ?? null, error: entry?.error ?? null };
}
