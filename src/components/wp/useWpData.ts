import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { toAppError, type AppError } from "../../api";
import { selectActiveSession, useStore } from "../../state/store";

type Loader<T> = (sessionId: string, database: string) => Promise<T>;

interface Result<T> {
  /** `sessionId/database` the reply belongs to. */
  source: string;
  data: T | null;
  error: AppError | null;
}

/** Loads data for the active session and database; `key` lists every other input `load` reads. */
export function useWpData<T>(key: readonly unknown[], load: Loader<T>) {
  const session = useStore(selectActiveSession);
  const version = useStore((s) => s.dataVersion);
  const [result, setResult] = useState<Result<T> | null>(null);
  const [loading, setLoading] = useState(true);
  const [nonce, setNonce] = useState(0);
  const latestLoad = useRef(load);
  const sessionId = session?.id;
  const database = session?.database;
  const scope = sessionId && database ? `${sessionId}/${database}` : null;
  const keyString = JSON.stringify(key);

  useLayoutEffect(() => {
    latestLoad.current = load;
  });

  useEffect(() => {
    if (!sessionId || !database) {
      setLoading(false);
      return;
    }
    const source = `${sessionId}/${database}`;
    let alive = true;
    setLoading(true);
    latestLoad.current(sessionId, database).then(
      (data) => {
        if (!alive) return;
        setResult({ source, data, error: null });
        setLoading(false);
      },
      (e) => {
        if (!alive) return;
        setResult((prev) => ({ source, data: prev?.source === source ? prev.data : null, error: toAppError(e) }));
        setLoading(false);
      },
    );
    return () => {
      alive = false;
    };
  }, [sessionId, database, version, nonce, keyString]);

  const reload = useCallback(() => setNonce((n) => n + 1), []);
  // Another database's reply is never shown, not even while the new one loads.
  const current = result && result.source === scope ? result : null;
  return { data: current?.data ?? null, error: current?.error ?? null, loading, reload };
}
