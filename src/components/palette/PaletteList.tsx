import { useEffect, useRef, type RefObject } from "react";
import { CornerDownLeft } from "lucide-react";
import type { PaletteHit } from "../../api";
import { KeyCap } from "../Field";
import { useValueHover } from "../value/useValueHover";
import { Value } from "../value/Value";
import type { Prefix } from "./parse";
import type { Item } from "./usePaletteItems";

interface Props {
  items: Item[];
  active: number;
  activeRef: RefObject<HTMLLIElement | null>;
  empty: string;
  onHover: (i: number) => void;
  onChoose: (i: number, alt: boolean) => void;
}

/** `wp_wc_orders #4711 wc-processing · ppcp-gateway · 49.90`: the source, the key, the summary. */
function HitRow({ hit }: { hit: PaletteHit }) {
  return (
    <span className="palette-hit mono">
      <span className="tn">{hit.source}</span>
      <span className="key">
        #
        {hit.title.map((value, i) => (
          <Value key={i} value={value} density="grid" />
        ))}
      </span>
      <span className="palette-detail">
        {hit.detail.map((value, i) => (
          <span key={i}>
            {i > 0 && <span className="o3"> · </span>}
            <Value value={value} density="grid" />
          </span>
        ))}
      </span>
    </span>
  );
}

function PlainRow({ item }: { item: Item }) {
  return (
    <>
      {item.badge && <span className="palette-kind">{item.badge}</span>}
      <span className="grow-text">
        {item.title}
        {item.subtitle && <span className="muted"> · {item.subtitle}</span>}
        {item.note && <span className="o2 palette-note"> {item.note}</span>}
      </span>
      <KeyCap keys={item.binding ?? ""} />
    </>
  );
}

export function PaletteList({ items, active, activeRef, empty, onHover, onChoose }: Props) {
  // The palette sits outside the app's hover area; its hits light their tables in the sidebar too.
  const listRef = useRef<HTMLUListElement>(null);
  useValueHover(listRef);
  useEffect(() => {
    activeRef.current?.scrollIntoView?.({ block: "nearest" });
  }, [active, items, activeRef]);

  let lastSection: string | null = null;
  return (
    <ul className="palette-list" role="listbox" aria-label="Results" ref={listRef}>
      {items.map((it, i) => {
        const header = it.section !== lastSection ? it.section : null;
        lastSection = it.section;
        return (
          <li
            key={it.key}
            ref={i === active ? activeRef : undefined}
            data-stage={`palette:item:${it.key}`}
            data-stage-list="palette:results"
            data-stage-label={it.title}
            data-row-table={it.hit && "table" in it.hit.address.target ? it.hit.address.target.table : undefined}
            role="option"
            aria-selected={i === active}
            aria-label={it.hit ? it.title : undefined}
            className={i === active ? "active" : ""}
            onMouseEnter={() => onHover(i)}
            onClick={(e) => onChoose(i, e.metaKey)}
          >
            {header && <span className="palette-section">{header}</span>}
            {it.hit ? <HitRow hit={it.hit} /> : <PlainRow item={it} />}
            {i === active && <CornerDownLeft size="0.923rem" className="muted" />}
          </li>
        );
      })}
      {items.length === 0 && <li className="muted">{empty}</li>}
    </ul>
  );
}

const PREFIX_HINTS = "# id · @ people · t: tables · q: queries · > commands · = find everywhere · databee://";

/** The keys, the prefixes and, for `#` without entities, which tables it reads. */
export function PaletteFooter({ prefix, hasEntities, hasHit }: { prefix: Prefix; hasEntities: boolean; hasHit: boolean }) {
  return (
    <div className="palette-foot o3">
      <span>{hasHit ? "↵ open in column · ⌘↵ new tab · ⌥ peek" : "↵ open · ⌘↵ new tab"}</span>
      {prefix === "#" && !hasEntities && <span>no entities: # reads tables under 1M estimated rows</span>}
      <span className="mono">{PREFIX_HINTS}</span>
    </div>
  );
}
