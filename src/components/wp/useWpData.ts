import { useCallback, useEffect, useState } from "react";
import { toAppError, type AppError } from "../../api";
import { selectActiveSession, useStore } from "../../state/store";

/**
 * Loads data for the active session/database and reloads when inputs change or the
 * store's `dataVersion` is bumped. Keeps the previous data while reloading.
 */
export function useWpData<T>(load: (sessionId: string, database: string) => Promise<T>, deps: unknown[]) {
  const session = useStore(selectActiveSession);
  const version = useStore((s) => s.dataVersion);
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [loading, setLoading] = useState(true);
  const [nonce, setNonce] = useState(0);
  const sessionId = session?.id;
  const database = session?.database;

  useEffect(() => {
    if (!sessionId || !database) return;
    let alive = true;
    setLoading(true);
    load(sessionId, database).then(
      (d) => {
        if (!alive) return;
        setData(d);
        setError(null);
        setLoading(false);
      },
      (e) => {
        if (!alive) return;
        setError(toAppError(e));
        setLoading(false);
      },
    );
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId, database, version, nonce, ...deps]);

  const reload = useCallback(() => setNonce((n) => n + 1), []);
  return { data, error, loading, reload };
}
