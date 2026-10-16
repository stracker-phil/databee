import { useEffect, useState } from "react";
import { Copy, X } from "lucide-react";
import { api, type DecodedValue } from "../api";
import { cellText } from "../lib/format";
import { useStore } from "../state/store";
import { ValueTree } from "./ValueTree";

const MAX_DECODE_CHARS = 2_000_000;

/** PHP-serialized or JSON text decoded by the backend; null for plain values. */
function useDecoded(text: string | null): DecodedValue | null {
  const [decoded, setDecoded] = useState<DecodedValue | null>(null);
  useEffect(() => {
    setDecoded(null);
    if (!text || text.length > MAX_DECODE_CHARS) return;
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
  const nullLabel = useStore((s) => s.settings?.nullLabel ?? "NULL");
  const { selectCell, toast } = useStore.getState();
  const [showRaw, setShowRaw] = useState(false);
  const textual = selected && (selected.cell.t === "text" || selected.cell.t === "json") ? cellText(selected.cell) : null;
  const decoded = useDecoded(textual);
  if (!selected) return null;
  const { cell, column } = selected;
  const raw = cellText(cell);

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
          onClick={() => void navigator.clipboard.writeText(raw).then(() => toast("success", "Copied value"))}
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
          <pre>{`${cell.v.len} bytes\n0x${cell.v.hexPreview}${cell.v.len > 16 ? "…" : ""}`}</pre>
        ) : decoded && !showRaw ? (
          <ValueTree value={decoded} />
        ) : (
          <>
            <pre>{raw}</pre>
            <div className="hint" style={{ marginTop: 8 }}>{raw.length.toLocaleString("en-US")} characters</div>
          </>
        )}
      </div>
    </aside>
  );
}
