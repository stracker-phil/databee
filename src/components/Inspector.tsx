import { Copy, X } from "lucide-react";
import { cellText } from "../lib/format";
import { useStore } from "../state/store";

function pretty(text: string): string | null {
  const trimmed = text.trim();
  if (!(trimmed.startsWith("{") || trimmed.startsWith("["))) return null;
  try {
    return JSON.stringify(JSON.parse(trimmed), null, 2);
  } catch {
    return null;
  }
}

export function Inspector() {
  const selected = useStore((s) => s.selectedCell);
  const nullLabel = useStore((s) => s.settings?.nullLabel ?? "NULL");
  const { selectCell, toast } = useStore.getState();
  if (!selected) return null;
  const { cell, column } = selected;
  const raw = cellText(cell);
  const json = cell.t === "json" || cell.t === "text" ? pretty(raw) : null;

  return (
    <aside className="inspector" aria-label="Value inspector">
      <div className="inspector-head">
        <span className="col" title={column.name}>{column.name}</span>
        <span className="badge">{column.typeName.toLowerCase()}</span>
        <div className="spacer" />
        <button
          className="icon-btn"
          aria-label="Copy value"
          onClick={() => void navigator.clipboard.writeText(raw).then(() => toast("success", "Copied value"))}
        >
          <Copy size={13} />
        </button>
        <button className="icon-btn" aria-label="Close inspector" onClick={() => selectCell(null)}>
          <X size={13} />
        </button>
      </div>
      <div className="inspector-body">
        {cell.t === "null" ? (
          <span className="muted" style={{ fontStyle: "italic" }}>{nullLabel}</span>
        ) : cell.t === "bytes" ? (
          <pre>{`${cell.v.len} bytes\n0x${cell.v.hexPreview}${cell.v.len > 16 ? "…" : ""}`}</pre>
        ) : (
          <>
            {json && <div className="hint" style={{ marginBottom: 6 }}>JSON</div>}
            <pre>{json ?? raw}</pre>
            <div className="hint" style={{ marginTop: 8 }}>{raw.length.toLocaleString("en-US")} characters</div>
          </>
        )}
      </div>
    </aside>
  );
}
