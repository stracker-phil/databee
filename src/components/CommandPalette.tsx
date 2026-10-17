import { useRef, useState } from "react";
import { Search } from "lucide-react";
import { useStore } from "../state/store";
import { Modal } from "./Modal";
import { PaletteFooter, PaletteList } from "./palette/PaletteList";
import type { Prefix } from "./palette/parse";
import { usePaletteItems } from "./palette/usePaletteItems";
import { usePeekHeld } from "./palette/usePeekHeld";
import { PeekCard } from "./value/Peek";

/** ⌘K: rows by id, people, external ids and values, plus tables, commands and recent items (docs/frontend.md). */
export function CommandPalette() {
  const open = useStore((s) => s.paletteOpen);
  // Mounted per opening, so the query, results and selection start fresh each time.
  return open ? <PaletteSheet /> : null;
}

function emptyText(prefix: Prefix, needle: string, rowSearch: boolean): string {
  if (prefix === "q:") return "No saved queries.";
  if (prefix === "#" && !rowSearch) return "Type an id, e.g. #4711.";
  if (prefix === "databee://") return "Not an address yet.";
  if ((prefix === "@" || prefix === "=") && !rowSearch) return "Type at least 2 characters.";
  return needle ? "Nothing found." : "Type to search…";
}

function PaletteSheet() {
  const seed = useStore((s) => s.paletteSeed);
  const { setPaletteOpen } = useStore.getState();
  const [query, setQuery] = useState(seed);
  const [index, setIndex] = useState(0);
  const activeRef = useRef<HTMLLIElement>(null);
  const { items, searching, prefix, needle, rowSearch, hasEntities } = usePaletteItems(query);
  // Results can shrink under the selection (a search reply, a narrower query).
  const active = Math.max(0, Math.min(index, items.length - 1));
  const peek = usePeekHeld(items[active], activeRef);

  const choose = (i: number, alt = false) => {
    const item = items[i];
    (alt ? (item?.goAlt ?? item?.go) : item?.go)?.();
  };

  return (
    <Modal onClose={() => setPaletteOpen(false)} className="palette" label="Command palette" stageWindow="palette">
      <div className="palette-input">
        <Search size="1.077rem" aria-hidden="true" />
        <input
          autoFocus
          aria-label="Search everything"
          data-stage="palette:search"
          placeholder="Order ID, email, PayPal ID, table, command…"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setIndex(0);
          }}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") (e.preventDefault(), setIndex(Math.min(items.length - 1, active + 1)));
            if (e.key === "ArrowUp") (e.preventDefault(), setIndex(Math.max(0, active - 1)));
            if (e.key === "Enter") (e.preventDefault(), choose(active, e.metaKey));
          }}
        />
        {searching && <span className="muted small">searching…</span>}
      </div>
      <PaletteList items={items} active={active} activeRef={activeRef} empty={emptyText(prefix, needle, rowSearch)} onHover={setIndex} onChoose={choose} />
      <PaletteFooter prefix={prefix} hasEntities={hasEntities} hasHit={!!items[active]?.hit} />
      {peek && <PeekCard target={peek} />}
    </Modal>
  );
}
