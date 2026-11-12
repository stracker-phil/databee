import type { MouseEvent, ReactNode } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import type { Address } from "../../api";
import { formatAddress } from "../../lib/address";
import { useStore } from "../../state/store";
import type { useCollapsedGroups } from "../object/useCollapsedGroups";

export const openAddress = (address: Address) => void useStore.getState().openAddress(formatAddress(address));

export const rowTableOf = (address: Address | null) => (address && "table" in address.target ? address.target.table : undefined);

export type Collapse = ReturnType<typeof useCollapsedGroups>;

interface SectionProps {
  id: string;
  label: string;
  name: string;
  count?: string;
  hint?: string;
  collapse: Collapse;
  children: ReactNode;
}

/** A collapsible section band: chevron, name, hint, count. */
export function Section({ id, label, name, count, hint, collapse, children }: SectionProps) {
  const collapsed = collapse.isCollapsed(id);
  const toggleLabel = `${collapsed ? "Expand" : "Collapse"} ${name}`;
  const onBandClick = (e: MouseEvent) => {
    if (!(e.target as HTMLElement).closest("button")) collapse.toggle(id);
  };
  return (
    <section aria-label={label} className="readout-section">
      <div className="sh" onClick={onBandClick}>
        <button className="sh-toggle" aria-expanded={!collapsed} aria-label={toggleLabel} data-stage={`readout:collapse:${id}`} data-stage-label={toggleLabel} onClick={() => collapse.toggle(id)}>
          {collapsed ? <ChevronRight size="0.923rem" /> : <ChevronDown size="0.923rem" />}
        </button>
        <span className="tn sans">{name}</span>
        {hint && <span className="via">{hint}</span>}
        <span className="spacer" />
        {count && <span className="cnt">{count}</span>}
      </div>
      {!collapsed && children}
    </section>
  );
}

/** An error in place of what failed: a row of its own (`row`), or text after the line's content. */
export function LineError({ error, row = false }: { error: string | null; row?: boolean }) {
  if (!error) return null;
  return row ? <div className="row line-error">{error}</div> : <span className="line-error"> · {error}</span>;
}
