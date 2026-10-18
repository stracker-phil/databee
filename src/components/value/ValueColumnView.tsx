import { useState } from "react";
import { Copy, X } from "lucide-react";
import { api, type ChangeRequest, type NewValue } from "../../api";
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

/** Plans the edited text as a `setCell` on the target row; returns that text once it is queued. */
async function queueEdit(target: EditTarget, text: string, path: number[], next: NewValue): Promise<string | null> {
  try {
    const raw = await api.editSerialized(text, path, next);
    const queued = await useStore.getState().queueChange(setCellChange(target, raw));
    return queued ? raw : null;
  } catch (e) {
    fail(e, "Could not edit the value");
    return null;
  }
}

interface Props {
  column: ValueColumn;
  onClose: () => void;
  /** The edit was queued; the column shows the edited text from now on. */
  onEdited: (text: string) => void;
}

export function ValueColumnView({ column, onClose, onEdited }: Props) {
  const [showRaw, setShowRaw] = useState(false);
  const decoded = useDecoded(column.text);
  const source = column.value.source;
  const target = editTarget(column);
  const onEdit = (path: number[], next: NewValue) => {
    if (!target) return;
    void queueEdit(target, column.text, path, next).then((raw) => raw !== null && onEdited(raw));
  };
  return (
    <div className="value-view" data-src={source ? sourceText(source) : undefined}>
      <div className="value-head">
        <span className="tn">{source ? `${source.table}.${source.column}` : "value"}</span>
        {source && <span className="key">{source.row.map((p) => p.value).join(", ")}</span>}
        <span className="spacer" />
        {decoded?.root && (
          <div className="segmented" role="group" aria-label="Show as">
            <button data-stage="value-column:tree" aria-pressed={!showRaw} onClick={() => setShowRaw(false)}>{decoded.format === "php" ? "PHP" : "JSON"}</button>
            <button data-stage="value-column:raw" aria-pressed={showRaw} onClick={() => setShowRaw(true)}>Raw</button>
          </div>
        )}
        <button className="icon-btn" aria-label="Copy value" data-stage="value-column:copy" onClick={() => void copyText(column.text, "value")}>
          <Copy size="1rem" />
        </button>
        <PaneMaximize />
        <button className="icon-btn" aria-label="Close value column" data-stage="value-column:close" onClick={onClose}>
          <X size="1rem" />
        </button>
      </div>
      <div className="value-body">
        {decoded?.root && !showRaw ? (
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
