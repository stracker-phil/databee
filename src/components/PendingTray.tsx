import { useState } from "react";
import { ChevronDown, ChevronRight, RotateCcw, X } from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import { selectActiveSession, useStore } from "../state/store";

/** Edits wait here with their SQL until applied; the last applied batch can be undone. */
export function PendingTray() {
  const session = useStore(selectActiveSession);
  const pending = useStore(useShallow((s) => s.pending.filter((p) => p.sessionId === s.activeSessionId)));
  const canUndo = useStore((s) => s.applied.some((a) => a.sessionId === s.activeSessionId));
  const { discardChange, discardAll, applyChanges, undoLast } = useStore.getState();
  const [open, setOpen] = useState<Record<number, boolean>>({});
  if (!session || (pending.length === 0 && !canUndo)) return null;

  return (
    <section className="tray" aria-label="Pending changes">
      <header className="tray-head">
        <strong>{pending.length ? `${pending.length} pending ${pending.length === 1 ? "change" : "changes"}` : "No pending changes"}</strong>
        <span className="muted small">Raw SQL writes: no WordPress hooks run.</span>
        <div className="spacer" />
        {canUndo && (
          <button className="btn" onClick={() => void undoLast()}>
            <RotateCcw size="0.923rem" /> Undo last apply
          </button>
        )}
        {pending.length > 0 && (
          <>
            <button className="btn" onClick={discardAll}>Discard all</button>
            <button className="btn btn-primary" onClick={() => void applyChanges()}>Apply (⌘S)</button>
          </>
        )}
      </header>
      {pending.length > 0 && (
        <ul className="tray-list">
          {pending.map((p) => (
            <li key={p.id}>
              <div className="tray-row">
                <button className="icon-btn tiny" aria-label="Show SQL" onClick={() => setOpen((o) => ({ ...o, [p.id]: !o[p.id] }))}>
                  {open[p.id] ? <ChevronDown size="0.846rem" /> : <ChevronRight size="0.846rem" />}
                </button>
                <span className="grow-text">{p.description}</span>
                {p.undo.length === 0 && <span className="badge" title="No automatic undo">no undo</span>}
                <button className="icon-btn tiny" aria-label={`Discard ${p.description}`} onClick={() => discardChange(p.id)}>
                  <X size="0.846rem" />
                </button>
              </div>
              {open[p.id] && <pre className="tray-sql">{p.statements.join(";\n")};</pre>}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
