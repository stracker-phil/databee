import { useCallback } from "react";
import type { Address } from "../../api";
import { collapseKey } from "../../lib/document";
import { selectCollapsedGroups, selectSessionBySlug, useStore } from "../../state/store";

/**
 * The collapsed groups of documents of the owner's table, remembered per connection and database
 * (`UiState.collapsedGroups`), so collapsing a group on one row collapses it on every row of that table.
 */
export function useCollapsedGroups(owner: Address) {
  const identity = useStore((s) => selectSessionBySlug(s, owner.connection)?.identity ?? null);
  const keys = useStore((s) => selectCollapsedGroups(s, owner));
  const isCollapsed = (groupId: string) => keys.includes(collapseKey(owner, groupId));
  const toggle = useCallback(
    (groupId: string) => {
      if (identity) useStore.getState().toggleGroupCollapsed(identity, owner.database, collapseKey(owner, groupId));
    },
    [identity, owner],
  );
  return { isCollapsed, toggle };
}
