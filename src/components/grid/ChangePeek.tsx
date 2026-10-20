import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import type { Cell } from "../../api";
import { cellDisplay } from "../../lib/format";
import { selectNullLabel, useStore } from "../../state/store";

const MAX_CHARS = 300;
const GAP = 4;

/** Whether ⌥ is held alone, as for a peek (`value/Peek.tsx`); a window that loses focus counts as released. */
function useAltKey(initiallyHeld: boolean): boolean {
  const [held, setHeld] = useState(initiallyHeld);
  useEffect(() => {
    const onDown = (e: KeyboardEvent) => {
      if (e.key === "Alt" && !e.metaKey && !e.ctrlKey && !e.shiftKey) setHeld(true);
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
  return held;
}

interface Props {
  /** ⌥ was held when the pointer entered the cell. */
  altHeld: boolean;
  column: string;
  shown: Cell;
  stored: Cell;
  /** The hovered cell's box; the overlay opens below it, or above it near the window's bottom. */
  anchor: DOMRect;
}

/** While ⌥ is held over a changed cell: its value as the page shows it → as the database holds it now. */
export function ChangePeek({ altHeld, column, shown, stored, anchor }: Props) {
  const held = useAltKey(altHeld);
  const nullLabel = useStore(selectNullLabel);
  if (!held) return null;
  const below = anchor.bottom + GAP;
  const style = below > window.innerHeight * 0.7 ? { left: anchor.left, bottom: window.innerHeight - anchor.top + GAP } : { left: anchor.left, top: below };
  return createPortal(
    <div className="db-peek" role="tooltip" style={style}>
      <div className="db-peek-head">
        <span className="mono">{column}</span> changed in the database
      </div>
      <div className="db-peek-values mono">
        <span className="o3">{cellDisplay(shown, nullLabel, MAX_CHARS)}</span>
        <span className="o3"> → </span>
        <span>{cellDisplay(stored, nullLabel, MAX_CHARS)}</span>
      </div>
    </div>,
    document.body,
  );
}
