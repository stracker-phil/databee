import { ChevronDown, ChevronRight } from "lucide-react";
import { memo, type MouseEvent } from "react";
import type { Group } from "../../api";
import { formatAddress } from "../../lib/address";
import { countText, groupOriginText, relationText } from "../../lib/document";
import { useStore } from "../../state/store";
import { TableIcon } from "../TableIcon";

/** Opens the group's rows in a table tab: its exact rows when an address names them, else the whole table. */
function openGroupRows(group: Group, database: string) {
  const s = useStore.getState();
  if (group.rowsAddress) return void s.openAddress(formatAddress(group.rowsAddress), { preview: true });
  s.openTableTab(database, group.table, "data", undefined, { preview: true });
}

interface Props {
  group: Group;
  database: string;
  anchor?: string;
  collapsed: boolean;
  onToggle: (groupId: string) => void;
}

/** Its `data-table` links the sidebar entry of the same table; hovering that entry lights the band (`TableFocus`). */
export const GroupHeader = memo(function GroupHeader({ group, database, anchor, collapsed, onToggle }: Props) {
  const events = group.via.kind === "events";
  const name = events ? "events" : group.table;
  const toggleLabel = `${collapsed ? "Expand" : "Collapse"} ${name}`;
  const onBandClick = (e: MouseEvent) => {
    if (!(e.target as HTMLElement).closest("button")) onToggle(group.id);
  };
  return (
    <div
      className="sh"
      data-table={group.table || undefined}
      data-src={groupOriginText(group) ?? undefined}
      onClick={onBandClick}
    >
      <button
        className="sh-toggle"
        aria-expanded={!collapsed}
        aria-label={toggleLabel}
        data-stage={`collapse:${group.id}`}
        data-stage-label={toggleLabel}
        onClick={() => onToggle(group.id)}
      >
        {collapsed ? <ChevronRight size="0.923rem" /> : <ChevronDown size="0.923rem" />}
      </button>
      {events ? (
        <span className="tn sans">{name}</span>
      ) : (
        <button className="tn" data-stage={anchor} data-stage-label={name} data-stage-list="groups" title="Open these rows in a table tab" onClick={() => openGroupRows(group, database)}>
          <TableIcon database={database} table={group.table} effect="go" />
          {name}
        </button>
      )}
      <span className="via">{relationText(group)}</span>
      <span className="spacer" />
      <span className="cnt">{countText(group)}</span>
    </div>
  );
});
