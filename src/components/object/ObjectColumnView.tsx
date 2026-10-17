/** One object column of a stack: its row's header block, then its Document or Rows view. */
import { useCallback, useEffect, useState } from "react";
import type { Provenance } from "../../api";
import type { ObjectColumn } from "../../lib/columns";
import { documentText, objectTitle, type ObjectView } from "../../lib/document";
import { useStore, type ObjectTab } from "../../state/store";
import { ErrorPanel } from "../ErrorPanel";
import { DocumentView } from "./DocumentView";
import { ObjectHeader } from "./ObjectHeader";
import { RowsView } from "./RowsView";
import { useDocument } from "./useDocument";
import { useViewScroll } from "./useViewScroll";

/** Once the first column's document names its entity, the tab says `order #4711` instead of the table name. */
function useEntityTitle(tab: ObjectTab, column: ObjectColumn, isRoot: boolean, entity: string | null | undefined) {
  useEffect(() => {
    if (!isRoot || !entity) return;
    const title = objectTitle(column.address, entity);
    if (title !== tab.title) useStore.getState().updateObjectTab(tab.id, { title });
  }, [tab.id, tab.title, column.address, isRoot, entity]);
}

interface Props {
  tab: ObjectTab;
  column: ObjectColumn;
  index: number;
  /** The tab is shown and this is its active column, so `e` edits here. */
  editable: boolean;
}

export function ObjectColumnView({ tab, column, index, editable }: Props) {
  const { document, error } = useDocument(tab.sessionId, documentText(column.address), column.address.database);
  const [scrollTo, setScrollTo] = useState<string | null>(null);
  const scroll = useViewScroll(column.view, !!document);
  useEntityTitle(tab, column, index === 0, document?.entity);

  const onFocus = useCallback((focus: Provenance | null) => useStore.getState().focusObject(tab.id, focus, index), [tab.id, index]);
  const setView = useCallback((view: ObjectView) => useStore.getState().setObjectView(tab.id, view, undefined, index), [tab.id, index]);
  const onMore = useCallback(
    (groupId: string) => {
      setScrollTo(groupId);
      setView("rows");
    },
    [setView],
  );
  useEffect(() => {
    if (column.view === "document") setScrollTo(null);
  }, [column.view]);

  if (error && !document) return <ErrorPanel message={error.message} code={error.code} />;
  if (!document) return <div className="empty-state">Loading…</div>;
  return (
    <div className="object" ref={scroll.ref} onScroll={scroll.onScroll}>
      <ObjectHeader document={document} owner={column.address} view={{ current: column.view, onChange: setView }} />
      {column.view === "document" ? (
        <DocumentView document={document} owner={column.address} focus={column.focus} onFocus={onFocus} onMore={onMore} editable={editable} />
      ) : (
        <RowsView document={document} owner={column.address} focus={column.focus} scrollTo={scrollTo} />
      )}
    </div>
  );
}
