import { useCallback, useEffect, useMemo, useState } from "react";
import { api, type Provenance } from "../../api";
import type { ObjectColumn } from "../../lib/columns";
import { compareMarks } from "../../lib/diff";
import { formatAddress } from "../../lib/address";
import { formatCount } from "../../lib/format";
import { objectTitle, type ObjectView } from "../../lib/document";
import { useApiResult } from "../../lib/useApiResult";
import { useStore, type ObjectTab } from "../../state/store";
import { ErrorPanel } from "../ErrorPanel";
import { DocumentView } from "./DocumentView";
import { ObjectHeader } from "./ObjectHeader";

function useComparison(sessionId: string, address: string) {
  const dataVersion = useStore((s) => s.dataVersion);
  const key = JSON.stringify([sessionId, address, dataVersion]);
  const { data: comparison, error } = useApiResult(key, () => api.compare(sessionId, address), { keepData: true });
  return { comparison, error };
}

interface Props {
  tab: ObjectTab;
  column: ObjectColumn;
  index: number;
  editable: boolean;
}

const noMore = () => {};

export function CompareView({ tab, column, index, editable }: Props) {
  const { comparison, error } = useComparison(tab.sessionId, formatAddress(column.address));
  const [rightFocus, setRightFocus] = useState<Provenance | null>(null);
  const left = useMemo(() => comparison && compareMarks(comparison.delta, "left"), [comparison]);
  const right = useMemo(() => comparison && compareMarks(comparison.delta, "right"), [comparison]);
  useEffect(() => {
    if (comparison) useStore.getState().keepDocument(tab.sessionId, comparison.left);
  }, [tab.sessionId, comparison]);
  const onFocus = useCallback((focus: Provenance | null) => useStore.getState().focusObject(tab.id, focus, index), [tab.id, index]);
  const setView = (view: ObjectView) => useStore.getState().setObjectView(tab.id, view, undefined, index);
  const place = { tabId: tab.id, index };

  if (error && !comparison) return <ErrorPanel message={error.message} code={error.code} />;
  if (!comparison) return <div className="empty-state">Loading…</div>;
  const { delta } = comparison;
  const differences = delta.changed.length + delta.added.length + delta.removed.length;
  const view = { current: "document" as const, onChange: setView };
  return (
    <div className="compare" data-stage="compare" data-stage-label="Compare">
      <div className="compare-summary o2">
        {formatCount(differences)} {differences === 1 ? "difference" : "differences"} · groups by rule, meta rows by key, other rows by position
      </div>
      <div className="compare-sides">
        <div className="object compare-side" role="group" aria-label={objectTitle(comparison.left.address)}>
          <ObjectHeader document={comparison.left} owner={comparison.left.address} view={view} place={place} />
          <DocumentView document={comparison.left} owner={comparison.left.address} focus={column.focus} onFocus={onFocus} onMore={noMore} editable={editable} diff={left} />
        </div>
        <div className="object compare-side" role="group" aria-label={objectTitle(comparison.right.address)}>
          <ObjectHeader document={comparison.right} owner={comparison.right.address} view={view} />
          <DocumentView document={comparison.right} owner={comparison.right.address} focus={rightFocus} onFocus={setRightFocus} onMore={noMore} editable={false} diff={right} />
        </div>
      </div>
    </div>
  );
}
