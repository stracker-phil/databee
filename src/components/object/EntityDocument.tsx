import { useCallback, useState } from "react";
import type { Provenance } from "../../api";
import { useStore } from "../../state/store";
import { ErrorPanel } from "../ErrorPanel";
import { DocumentView } from "./DocumentView";
import { ObjectHeader } from "./ObjectHeader";
import { useDocument } from "./useDocument";

/**
 * The document of an entity record (`order 4711`) outside a tab: the WordPress list's side pane. `+n more`
 * opens the object tab in its Rows view.
 */
export function EntityDocument({ sessionId, database, entity, id }: { sessionId: string; database: string; entity: string; id: string }) {
  const { document, error } = useDocument(sessionId, `${entity} ${id}`, database);
  const [focus, setFocus] = useState<Provenance | null>(null);
  const onFocus = useCallback((source: Provenance | null) => setFocus(source), []);
  const onMore = useCallback(() => {
    if (document) useStore.getState().openObject({ ...document.address, view: "rows" });
  }, [document]);
  if (error && !document) return <ErrorPanel message={error.message} code={error.code} />;
  if (!document) return <div className="empty-state">Loading…</div>;
  return (
    <div className="object">
      <ObjectHeader document={document} owner={document.address} />
      <DocumentView document={document} owner={document.address} focus={focus} onFocus={onFocus} onMore={onMore} editable={false} />
    </div>
  );
}
