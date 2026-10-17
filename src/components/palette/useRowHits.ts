import { useEffect, useState } from "react";
import { api, type PaletteHit } from "../../api";
import { selectActiveSession, useStore } from "../../state/store";
import { searchesRows, searchText, type Prefix } from "./parse";

const DEBOUNCE_MS = 200;

/** Rust's row hits for the input (`palette_search`), debounced; a newer input drops older replies. */
export function useRowHits(prefix: Prefix, needle: string): { hits: PaletteHit[]; searching: boolean } {
  const session = useStore(selectActiveSession);
  const [hits, setHits] = useState<PaletteHit[]>([]);
  const [searching, setSearching] = useState(false);
  const wanted = !!session?.database && searchesRows(prefix, needle);
  const text = searchText(prefix, needle);

  useEffect(() => {
    if (!session?.database || !wanted) {
      setHits([]);
      return;
    }
    let alive = true;
    setSearching(true);
    const timer = setTimeout(() => {
      api.paletteSearch(session.id, session.database!, text).then(
        (found) => alive && (setHits(found), setSearching(false)),
        () => alive && (setHits([]), setSearching(false)),
      );
    }, DEBOUNCE_MS);
    return () => {
      alive = false;
      clearTimeout(timer);
      setSearching(false);
    };
  }, [text, wanted, session?.id, session?.database]);

  return { hits, searching };
}
