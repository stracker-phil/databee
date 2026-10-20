import { useCallback, useEffect, useMemo, useState } from "react";
import type { Provenance } from "../../api";
import type { ObjectColumn } from "../../lib/columns";
import { documentMarks } from "../../lib/diff";
import { documentText, objectTitle, type ObjectView } from "../../lib/document";
import { documentKey } from "../../state/slices/objects";
import { useStore, type ObjectTab } from "../../state/store";
import { ErrorPanel } from "../ErrorPanel";
import { ChangeList } from "./ChangeList";
import { CompareView } from "./CompareView";
import { DocumentView } from "./DocumentView";
import { ObjectHeader } from "./ObjectHeader";
import { RowsView } from "./RowsView";
import { useDocument } from "./useDocument";
import { useViewScroll } from "./useViewScroll";

function useEntityTitle(tab: ObjectTab, column: ObjectColumn, isRoot: boolean, entity: string | null | undefined) {
  useEffect(() => {
    if (!isRoot || !entity) return;
    const title = objectTitle(column.address, entity);
    if (title !== tab.title) useStore.getState().updateObjectTab(tab.id, { title });
  }, [tab.id, tab.title, column.address, isRoot, entity]);
}

/** The group a "more rows" link opened Rows at; forgotten once the column shows its document again. */
function useGroupJump(view: ObjectView, setView: (view: ObjectView) => void) {
  const [scrollTo, setScrollTo] = useState<string | null>(null);
  const [shownView, setShownView] = useState(view);
  if (view !== shownView) {
    setShownView(view);
    if (view === "document") setScrollTo(null);
  }
  const onMore = (groupId: string) => {
    setScrollTo(groupId);
    setView("rows");
  };
  return { scrollTo, onMore };
}

interface Props {
  tab: ObjectTab;
  column: ObjectColumn;
  index: number;
  /** The tab is shown and this is its active column, so `e` edits here. */
  editable: boolean;
}

function SingleColumnView({ tab, column, index, editable }: Props) {
  const text = documentText(column.address);
  const { document, error } = useDocument(tab.sessionId, text, column.address.database);
  const changes = useStore((s) => s.changeMarks[documentKey(tab.sessionId, text)]);
  const diff = useMemo(() => (changes ? documentMarks(changes) : null), [changes]);
  const scroll = useViewScroll(column.view, !!document);
  useEntityTitle(tab, column, index === 0, document?.entity);

  const onFocus = useCallback((focus: Provenance | null) => useStore.getState().focusObject(tab.id, focus, index), [tab.id, index]);
  const setView = (view: ObjectView) => useStore.getState().setObjectView(tab.id, view, undefined, index);
  const onJump = (source: Provenance) => useStore.getState().setObjectView(tab.id, "document", source, index);
  const { scrollTo, onMore } = useGroupJump(column.view, setView);

  if (error && !document) return <ErrorPanel message={error.message} code={error.code} />;
  if (!document) return <div className="empty-state">Loading…</div>;
  return (
    <div className="object" ref={scroll.ref} onScroll={scroll.onScroll}>
      <ObjectHeader document={document} owner={column.address} view={{ current: column.view, onChange: setView }} place={{ tabId: tab.id, index }} />
      {column.view === "document" ? (
        <DocumentView document={document} owner={column.address} focus={column.focus} onFocus={onFocus} onMore={onMore} editable={editable} diff={diff} />
      ) : (
        <RowsView document={document} owner={column.address} focus={column.focus} scrollTo={scrollTo} />
      )}
      {changes && <ChangeList marks={changes} dismissCommand="object.dismissChanges" onJump={onJump} />}
    </div>
  );
}

export function ObjectColumnView(props: Props) {
  if (props.column.address.compare && props.column.view === "document") return <CompareView {...props} />;
  return <SingleColumnView {...props} />;
}
