import { useEffect, useState, type RefObject } from "react";
import { formatAddress } from "../../lib/address";
import type { PeekTarget } from "../value/Peek";
import type { Item } from "./usePaletteItems";

const isBareAlt = (e: KeyboardEvent) => e.key === "Alt" && !e.repeat && !e.metaKey && !e.ctrlKey && !e.shiftKey;

/** While ⌥ is held alone, the selected row hit is peeked. */
export function usePeekHeld(item: Item | undefined, activeRef: RefObject<HTMLLIElement | null>): PeekTarget | null {
  const [held, setHeld] = useState(false);
  useEffect(() => {
    const onDown = (e: KeyboardEvent) => {
      if (isBareAlt(e)) setHeld(true);
    };
    const onUp = (e: KeyboardEvent) => {
      if (e.key === "Alt") setHeld(false);
    };
    const release = () => setHeld(false);
    window.addEventListener("keydown", onDown, true);
    window.addEventListener("keyup", onUp, true);
    window.addEventListener("blur", release);
    return () => {
      window.removeEventListener("keydown", onDown, true);
      window.removeEventListener("keyup", onUp, true);
      window.removeEventListener("blur", release);
    };
  }, []);
  const hit = item?.hit;
  if (!held || hit?.address.target.kind !== "row") return null;
  return { address: formatAddress(hit.address), via: null, by: null, from: null, rect: activeRef.current?.getBoundingClientRect() ?? null, place: null };
}
