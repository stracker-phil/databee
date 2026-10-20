import { createPortal } from "react-dom";
import type { Cell } from "../../api";
import { cellDisplay } from "../../lib/format";
import { selectNullLabel, useStore } from "../../state/store";
import { useAltPeek } from "../value/altPeek";

const MAX_CHARS = 300;
const GAP = 4;

interface Props {
  column: string;
  shown: Cell;
  stored: Cell;
  /** The changed cell; the overlay opens below it, or above it near the window's bottom. */
  cell: Element;
}

/** The ⌥ peek of a changed cell: its value as the page shows it → as the database holds it now. */
export function ChangePeek({ column, shown, stored, cell }: Props) {
  const peeked = useAltPeek().target === cell;
  const nullLabel = useStore(selectNullLabel);
  if (!peeked) return null;
  const anchor = cell.getBoundingClientRect();
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
