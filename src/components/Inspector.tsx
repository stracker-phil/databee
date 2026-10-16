import { useEffect, useState } from "react";
import { Copy, X } from "lucide-react";
import { api, toAppError, type Cell, type DecodedValue } from "../api";
import { fullCellText } from "../lib/cells";
import { copyText } from "../lib/clipboard";
import { cellText, formatCount } from "../lib/format";
import { useStore, type SelectedCell } from "../state/store";
import { ValueTree } from "./ValueTree";

/** PHP-serialized or JSON text decoded by the backend; null for plain values. */
function useDecoded(text: string | null): DecodedValue | null {
  const [decoded, setDecoded] = useState<DecodedValue | null>(null);
  useEffect(() => {
    setDecoded(null);
    if (!text) return;
    let alive = true;
    api.decodeValue(text).then(
      (d) => alive && setDecoded(d),
      () => alive && setDecoded(null),
    );
    return () => {
      alive = false;
    };
  }, [text]);
  return decoded;
}

export function Inspector() {
  const selected = useStore((s) => s.selectedCell);
  if (!selected) return null;
  // A new cell starts in the decoded view again.
  return <CellInspector key={`${selected.tabId}:${selected.row}:${selected.col}`} selected={selected} />;
}

type FullText = { text: string } | { error: string };

/** The cell's text; a `longText` cell is fetched in full first (null while loading). */
function useFullText({ cell, source, row, col }: SelectedCell): FullText | null {
  const [fetched, setFetched] = useState<{ cell: Cell; full: FullText } | null>(null);
  useEffect(() => {
    if (cell.t !== "longText") return;
    let alive = true;
    fullCellText(cell, source, row, col).then(
      (text) => alive && setFetched({ cell, full: { text } }),
      (e) => alive && setFetched({ cell, full: { error: toAppError(e).message } }),
    );
    return () => {
      alive = false;
    };
  }, [cell, source, row, col]);
  if (cell.t !== "longText") return { text: cellText(cell) };
  return fetched?.cell === cell ? fetched.full : null;
}

function CellInspector({ selected }: { selected: SelectedCell }) {
  const nullLabel = useStore((s) => s.settings?.nullLabel ?? "NULL");
  const { selectCell } = useStore.getState();
  const [showRaw, setShowRaw] = useState(false);
  const { cell, column } = selected;
  const full = useFullText(selected);
  const raw = full && "text" in full ? full.text : null;
  const decodable = cell.t === "text" || cell.t === "json" || cell.t === "longText";
  const decoded = useDecoded(decodable ? raw : null);

  return (
    <aside className="inspector" aria-label="Value inspector" data-stage="inspector">
      <div className="inspector-head">
        <span className="col" title={column.name}>{column.name}</span>
        <span className="badge">{column.typeName.toLowerCase()}</span>
        <div className="spacer" />
        {decoded && (
          <div className="segmented">
            <button aria-pressed={!showRaw} onClick={() => setShowRaw(false)}>{decoded.format === "php" ? "PHP" : "JSON"}</button>
            <button aria-pressed={showRaw} onClick={() => setShowRaw(true)}>Raw</button>
          </div>
        )}
        <button
          className="icon-btn"
          aria-label="Copy value"
          disabled={raw === null}
          onClick={() => raw !== null && void copyText(raw, "value")}
        >
          <Copy size="1rem" />
        </button>
        <button data-stage="inspector:close" className="icon-btn" aria-label="Close inspector" onClick={() => selectCell(null)}>
          <X size="1rem" />
        </button>
      </div>
      <div className="inspector-body">
        {cell.t === "null" ? (
          <span className="muted" style={{ fontStyle: "italic" }}>{nullLabel}</span>
        ) : cell.t === "bytes" ? (
          <pre>{`${cell.v.len} bytes\n${raw}`}</pre>
        ) : full === null ? (
          <span className="muted">Loading the full value…</span>
        ) : "error" in full ? (
          <span className="muted">{full.error}</span>
        ) : decoded && !showRaw ? (
          <ValueTree value={decoded} />
        ) : (
          <>
            <pre>{full.text}</pre>
            <div className="hint" style={{ marginTop: 8 }}>{formatCount(full.text.length)} characters</div>
          </>
        )}
      </div>
    </aside>
  );
}
