import { memo } from "react";
import type { Group } from "../../api";
import { formatAddress } from "../../lib/address";
import { countText, originText, relationText } from "../../lib/document";
import { useStore } from "../../state/store";

/** Opens the group's rows in a table tab: its exact rows when an address names them, else the whole table. */
export function openGroupRows(group: Group, database: string) {
  const s = useStore.getState();
  if (group.rowsAddress) return void s.openAddress(formatAddress(group.rowsAddress), { preview: true });
  s.openTableTab(database, group.table, "data", undefined, { preview: true });
}

interface Props {
  group: Group;
  database: string;
  anchor?: string;
}

/**
 * The section band of a group (docs/ux.md, Section header band). Its `data-table` links the sidebar entry
 * of the same table; hovering that entry lights the band (`lit`).
 */
export const GroupHeader = memo(function GroupHeader({ group, database, anchor }: Props) {
  const lit = useStore((s) => s.hover.linked && !!group.table && s.hover.table === group.table);
  const events = group.via.kind === "events";
  const name = events ? "events" : group.table;
  return (
    <div className={`sh${lit ? " lit" : ""}`} data-table={group.table || undefined} data-src={originText(group) ?? undefined}>
      {events ? (
        <span className="tn sans">{name}</span>
      ) : (
        <button className="tn" data-stage={anchor} data-stage-label={name} data-stage-list="groups" title="Open these rows in a table tab" onClick={() => openGroupRows(group, database)}>
          {name}
        </button>
      )}
      <span className="via">{relationText(group)}</span>
      <span className="spacer" />
      <span className="cnt">{countText(group)}</span>
    </div>
  );
});
