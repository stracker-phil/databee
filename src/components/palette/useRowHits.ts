import { api, type PaletteHit } from "../../api";
import { useApiResult } from "../../lib/useApiResult";
import { fail, selectActiveSession, useStore } from "../../state/store";
import { searchesRows, searchText, type Prefix } from "./parse";

const DEBOUNCE_MS = 200;
const NO_HITS: PaletteHit[] = [];

/** Rust's row hits for the input (`palette_search`), debounced; a newer input drops older replies. */
export function useRowHits(prefix: Prefix, needle: string): { hits: PaletteHit[]; searching: boolean } {
  const session = useStore(selectActiveSession);
  const sessionId = session?.id ?? "";
  const database = session?.database ?? null;
  const lost = useStore((s) => !!s.health[sessionId]);
  const text = searchText(prefix, needle);
  // Row search reads the server; while the connection is lost the palette says so (`PaletteFooter`).
  const key = database && !lost && searchesRows(prefix, needle) ? JSON.stringify([sessionId, database, text]) : null;
  const result = useApiResult(key, () => api.paletteSearch(sessionId, database ?? "", text), {
    keepData: true,
    delayMs: DEBOUNCE_MS,
    onError: (e) => fail(e, "Could not search rows"),
  });
  if (key === null) return { hits: NO_HITS, searching: false };
  return { hits: result.state === "failed" ? NO_HITS : (result.data ?? NO_HITS), searching: result.state === "loading" };
}
