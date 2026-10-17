import type { Group } from "../../api";
import { isEmpty, relationText } from "../../lib/document";
import { openGroupRows } from "./GroupHeader";

/** Linked tables with no rows here; an empty `events` group is not a table and is left out. */
const emptyTables = (groups: Group[]) => groups.filter((g) => isEmpty(g) && g.via.kind !== "events");

export function AlsoLinked({ groups, database }: { groups: Group[]; database: string }) {
  const empty = emptyTables(groups);
  if (empty.length === 0) return null;
  return (
    <div className="also">
      Also linked:{" "}
      {empty.map((group, i) => (
        <span key={group.id}>
          {i > 0 && " · "}
          <button
            className="also-table"
            data-table={group.table}
            data-stage={`also:${group.id}`}
            data-stage-label={group.table}
            title={`${relationText(group)} · 0 rows`}
            onClick={() => openGroupRows(group, database)}
          >
            {group.table}
          </button>
        </span>
      ))}
    </div>
  );
}
