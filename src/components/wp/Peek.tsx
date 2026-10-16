import { useEffect, useRef, useState, type ReactNode } from "react";
import { api, type PeekFact } from "../../api";
import { relativeFromUnix } from "../../lib/wpFormat";
import { selectActiveSession, useStore } from "../../state/store";
import { StatusPill } from "./ui";

export const PEEK_DELAY_MS = 300;

/** Facts per `session/database/entity/id`, valid for one `dataVersion`. */
const cache = new Map<string, Promise<PeekFact[]>>();
let cachedVersion = 0;

export function clearPeekCache() {
  cache.clear();
}

function cachedPeek(sessionId: string, database: string, entity: string, id: string, version: number) {
  if (version !== cachedVersion) {
    cache.clear();
    cachedVersion = version;
  }
  const key = `${sessionId}/${database}/${entity}/${id}`;
  let facts = cache.get(key);
  if (!facts) {
    facts = api.wpPeek(sessionId, database, entity, id).catch(() => []);
    cache.set(key, facts);
  }
  return facts;
}

export function usePeek(entity: string, id: string, enabled: boolean): PeekFact[] | null {
  const session = useStore(selectActiveSession);
  const version = useStore((s) => s.dataVersion);
  const [facts, setFacts] = useState<PeekFact[] | null>(null);
  const sessionId = session?.id;
  const database = session?.database;
  useEffect(() => {
    if (!enabled || !sessionId || !database) return;
    let alive = true;
    void cachedPeek(sessionId, database, entity, id, version).then((f) => alive && setFacts(f));
    return () => {
      alive = false;
    };
  }, [enabled, entity, id, sessionId, database, version]);
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
