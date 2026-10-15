import { useEffect, useRef, useState, type ReactNode } from "react";
import { api, type PeekFact } from "../../api";
import { relativeFromUnix } from "../../lib/wpFormat";
import { selectActiveSession, useStore } from "../../state/store";
import { StatusPill } from "./ui";

export const PEEK_DELAY_MS = 300;

/** Facts per `session:entity:id`; a session's peeks don't change while it is open. */
const cache = new Map<string, Promise<PeekFact[]>>();

export function clearPeekCache() {
  cache.clear();
}

export function usePeek(entity: string, id: string, enabled: boolean): PeekFact[] | null {
  const session = useStore(selectActiveSession);
  const [facts, setFacts] = useState<PeekFact[] | null>(null);
  useEffect(() => {
    if (!enabled || !session?.database) return;
    const key = `${session.id}:${entity}:${id}`;
    let p = cache.get(key);
    if (!p) {
      p = api.wpPeek(session.id, session.database, entity, id).catch(() => []);
      cache.set(key, p);
    }
    let alive = true;
    void p.then((f) => alive && setFacts(f));
    return () => {
      alive = false;
    };
  }, [enabled, entity, id, session?.id, session?.database]);
  return facts;
}

export function PeekCard({ entity, id }: { entity: string; id: string }) {
  const facts = usePeek(entity, id, true);
  return (
    <div className="peek" role="tooltip">
      <div className="peek-title">{entity} #{id}</div>
      {facts == null && <div className="muted small">Loading…</div>}
      {facts && facts.length === 0 && <div className="muted small">Nothing to show.</div>}
      {facts && facts.length > 0 && (
        <dl className="peek-facts">
          {facts.map((f) => (
            <div key={f.label} className="peek-row">
              <dt>{f.label}</dt>
              <dd className={f.kind === "money" ? "money" : undefined}>
                {f.kind === "status" ? <StatusPill status={f.value} /> : f.kind === "time" ? relativeFromUnix(Number(f.value)) : f.value}
              </dd>
            </div>
          ))}
        </dl>
      )}
    </div>
  );
}

/** Shows a peek card after hovering the child for 300 ms. */
export function Peekable({ entity, id, children }: { entity: string; id: string; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const cancel = () => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    setOpen(false);
  };
  useEffect(() => () => void (timer.current && clearTimeout(timer.current)), []);
  return (
    <span
      className="peekable"
      onMouseEnter={() => {
        timer.current = setTimeout(() => setOpen(true), PEEK_DELAY_MS);
      }}
      onMouseLeave={cancel}
      onMouseDown={cancel}
    >
      {children}
      {open && <PeekCard entity={entity} id={id} />}
    </span>
  );
}
