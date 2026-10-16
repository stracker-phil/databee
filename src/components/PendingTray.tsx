import { useEffect, useRef, useState } from "react";
import { ChevronDown, ChevronRight, Loader2, RotateCcw, X } from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import { selectActivePending, selectActiveSession, selectCanUndo, useStore } from "../state/store";

/** Edits wait here with their SQL until applied; the last applied batch can be undone. */
export function PendingTray() {
  const session = useStore(selectActiveSession);
  const pending = useStore(useShallow(selectActivePending));
  const canUndo = useStore(selectCanUndo);
  const applying = useStore((s) => s.applying);
  const { discardChange, discardAll, applyChanges, undoLast } = useStore.getState();
  const [open, setOpen] = useState<Record<number, boolean>>({});
  const focusRequest = useStore((s) => s.trayFocusRequest);
  const applyRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (focusRequest) applyRef.current?.focus();
  }, [focusRequest]);
  if (!session || (pending.length === 0 && !canUndo)) return null;

  return (
    <section className="tray" aria-label="Pending changes" data-stage="tray">
      <header className="tray-head">
        <strong>{pending.length ? `${pending.length} pending ${pending.length === 1 ? "change" : "changes"}` : "No pending changes"}</strong>
        <span className="muted small">Raw SQL writes: no WordPress hooks run.</span>
        <div className="spacer" />
        {canUndo && (
          <button data-stage="tray:undo" className="btn" disabled={applying} aria-busy={applying} onClick={() => void undoLast()}>
            <RotateCcw size="0.923rem" /> Undo last apply
          </button>
        )}
        {pending.length > 0 && (
          <>
            <button data-stage="tray:discard" className="btn" onClick={() => void discardAll()}>Discard all</button>
            <button ref={applyRef} data-stage="tray:apply" className="btn btn-primary" disabled={applying} aria-busy={applying} onClick={() => void applyChanges()}>
              {applying && <Loader2 size="0.923rem" className="spin" />} Apply (⌘S)
            </button>
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
                {!p.canUndo && <span className="badge" title="No automatic undo">no undo</span>}
                <button className="icon-btn tiny" aria-label={`Discard ${p.description}`} onClick={() => void discardChange(p.id)}>
                  <X size="0.846rem" />
                </button>
              </div>
              {open[p.id] && (
                <ul className="tray-sql" aria-label="SQL">
                  {p.statements.map((sql, i) => (
                    <li key={i}>
                      <code>{sql}</code>
                    </li>
                  ))}
                </ul>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
