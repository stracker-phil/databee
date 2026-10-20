import { useEffect, useState } from "react";
import { Copy, X } from "lucide-react";
import { api, type ChangeRequest, type NewValue, type Provenance } from "../../api";
import type { ValueColumn } from "../../lib/columns";
import { copyText } from "../../lib/clipboard";
import { formatCount } from "../../lib/format";
import { sourceText } from "../../lib/provenance";
import { fail, useStore } from "../../state/store";
import { ValueTree } from "../ValueTree";
import { PaneMaximize } from "../object/PaneMaximize";
import { useDecoded } from "./useDecoded";

/** The row a value column's edits go to: its provenance row, when a single-column key names it. */
interface EditTarget {
  database: string;
  table: string;
  column: string;
  keyColumn: string;
  key: string;
}

function editTarget(column: ValueColumn): EditTarget | null {
  const source = column.value.source;
  if (source?.row.length !== 1 || !column.database) return null;
  const [key] = source.row;
  return { database: column.database, table: source.table, column: source.column, keyColumn: key.column, key: key.value };
}

const setCellChange = (target: EditTarget, value: string): ChangeRequest => ({ type: "setCell", ...target, value });

/** Plans the edited text as a `setCell` on the target row; returns that text and the plan's id once it is queued. */
async function queueEdit(target: EditTarget, text: string, path: number[], next: NewValue): Promise<{ raw: string; planId: number } | null> {
  try {
    const raw = await api.editSerialized(text, path, next);
    const planId = await useStore.getState().queueChange(setCellChange(target, raw));
    return planId === null ? null : { raw, planId };
  } catch (e) {
    fail(e, "Could not edit the value");
    return null;
  }
}

/** Where a value column is: column `index` of an object tab's stack, or the right column of a table or query tab (`index` null). */
export interface ValuePlace {
  tabId: string;
  index: number | null;
}

/** Reads the value again once the data changed since it was read (⌘R, an applied write), also when its pane shows again. */
function useCurrentValue(column: ValueColumn, place: ValuePlace) {
  const dataVersion = useStore((s) => s.dataVersion);
  const stale = column.version !== undefined && column.version < dataVersion;
  useEffect(() => {
    if (stale) void useStore.getState().reloadValueColumn(place.tabId, place.index);
  }, [stale, dataVersion, place.tabId, place.index]);
}

const rowText = (source: Provenance) => `${source.table} ${source.row.map((p) => `${p.column} = ${p.value}`).join(", ")}`;

interface Props {
  column: ValueColumn;
  place: ValuePlace;
  onClose: () => void;
}

export function ValueColumnView({ column, place, onClose }: Props) {
  const [showRaw, setShowRaw] = useState(false);
  const decoded = useDecoded(column.text);
  const source = column.value.source;
  const gone = column.missing ? source : null;
  const target = gone ? null : editTarget(column);
  useCurrentValue(column, place);
  const onEdit = (path: number[], next: NewValue) => {
    if (!target) return;
    void queueEdit(target, column.text, path, next).then((queued) => queued && useStore.getState().updateValueColumn(place.tabId, place.index, queued.raw, queued.planId));
  };
  return (
    <div className="value-view" data-src={source ? sourceText(source) : undefined}>
      <div className="value-head">
        <span className="tn">{source ? `${source.table}.${source.column}` : "value"}</span>
        {source && <span className="key">{source.row.map((p) => p.value).join(", ")}</span>}
        <span className="spacer" />
        {decoded?.root && !gone && (
          <div className="segmented" role="group" aria-label="Show as">
            <button data-stage="value-column:tree" aria-pressed={!showRaw} onClick={() => setShowRaw(false)}>{decoded.format === "php" ? "PHP" : "JSON"}</button>
            <button data-stage="value-column:raw" aria-pressed={showRaw} onClick={() => setShowRaw(true)}>Raw</button>
          </div>
        )}
        <button className="icon-btn" aria-label="Copy value" data-stage="value-column:copy" disabled={!!gone} onClick={() => void copyText(column.text, "value")}>
          <Copy size="1rem" />
        </button>
        <PaneMaximize />
        <button className="icon-btn" aria-label="Close value column" data-stage="value-column:close" onClick={onClose}>
          <X size="1rem" />
        </button>
      </div>
      <div className="value-body">
        {gone ? (
          <div className="empty-state" role="status">
            <span>
              Row <span className="mono">{rowText(gone)}</span> no longer exists
            </span>
          </div>
        ) : decoded?.root && !showRaw ? (
          <ValueTree value={decoded} onEdit={target ? onEdit : undefined} />
        ) : (
          <>
            <pre>{column.text}</pre>
            <div className="hint">{formatCount(column.text.length)} characters</div>
          </>
        )}
      </div>
    </div>
  );
}
