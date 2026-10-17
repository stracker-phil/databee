/** An object tab: the header block, then the Document or the Rows view of one row. */
import { useCallback, useEffect, useState } from "react";
import type { Provenance } from "../../api";
import { documentText, objectTitle, type ObjectView } from "../../lib/document";
import { useStore, type ObjectTab } from "../../state/store";
import { ErrorPanel } from "../ErrorPanel";
import { DocumentView } from "./DocumentView";
import { ObjectHeader } from "./ObjectHeader";
import { RowsView } from "./RowsView";
import { useDocument } from "./useDocument";
import { useViewScroll } from "./useViewScroll";

/** Once the document names its entity, the tab says `order #4711` instead of the table name. */
function useEntityTitle(tab: ObjectTab, entity: string | null | undefined) {
  useEffect(() => {
    if (!entity) return;
    const title = objectTitle(tab.address, entity);
    if (title !== tab.title) useStore.getState().updateObjectTab(tab.id, { title });
  }, [tab.id, tab.address, tab.title, entity]);
}

export function ObjectPanel({ tab }: { tab: ObjectTab }) {
  const { document, error } = useDocument(tab.sessionId, documentText(tab.address), tab.address.database);
  const active = useStore((s) => s.activeTabId === tab.id);
  const [scrollTo, setScrollTo] = useState<string | null>(null);
  const scroll = useViewScroll(tab.view, !!document);
  useEntityTitle(tab, document?.entity);

  const onFocus = useCallback((focus: Provenance | null) => useStore.getState().focusObject(tab.id, focus), [tab.id]);
  const setView = (view: ObjectView) => useStore.getState().setObjectView(tab.id, view);
  const onMore = useCallback(
    (groupId: string) => {
      setScrollTo(groupId);
      useStore.getState().setObjectView(tab.id, "rows");
    },
    [tab.id],
  );

  if (error && !document) return <ErrorPanel message={error.message} code={error.code} />;
  if (!document) return <div className="empty-state">Loading…</div>;
  return (
    <div className="object" ref={scroll.ref} onScroll={scroll.onScroll} data-stage="object" data-stage-label={tab.title}>
      <ObjectHeader document={document} owner={tab.address} view={{ current: tab.view, onChange: setView }} />
      {tab.view === "document" ? (
        <DocumentView document={document} owner={tab.address} focus={tab.focus} onFocus={onFocus} onMore={onMore} editable={active} />
      ) : (
        <RowsView document={document} owner={tab.address} focus={tab.focus} scrollTo={scrollTo} />
      )}
    </div>
  );
}
