import { useCallback } from "react";
import type { Address } from "../../api";
import { useStore } from "../../state/store";

const NONE: string[] = [];

const groupKey = (table: string, groupId: string) => `${table}::${groupId}`;

const documentTable = (owner: Address) => ("table" in owner.target ? owner.target.table : "");

/**
 * The collapsed groups of documents of the owner's table, remembered per connection and database
 * (`UiState.collapsedGroups`), so collapsing a group on one row collapses it on every row of that table.
 */
export function useCollapsedGroups(owner: Address) {
  const identity = useStore((s) => s.sessions.find((x) => x.slug === owner.connection)?.identity ?? null);
  const keys = useStore((s) => (identity ? s.uiState[identity]?.collapsedGroups?.[owner.database] : undefined) ?? NONE);
  const table = documentTable(owner);
  const isCollapsed = (groupId: string) => keys.includes(groupKey(table, groupId));
  const toggle = useCallback(
    (groupId: string) => {
      if (identity) useStore.getState().toggleGroupCollapsed(identity, owner.database, groupKey(table, groupId));
    },
    [identity, owner.database, table],
  );
  return { isCollapsed, toggle };
}
