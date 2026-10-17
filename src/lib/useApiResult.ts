import { useCallback, useEffect, useRef, useState } from "react";
import { toAppError, type AppError } from "../api";

type ApiResult<T> =
  | { state: "loading"; data: T | null; error: null }
  | { state: "loaded"; data: T; error: null }
  | { state: "failed"; data: T | null; error: AppError };

export interface ApiResultOptions {
  /** Called once per failed request of the current key; without it the caller shows `error` in place. */
  onError?: (error: AppError) => void;
  /** Keep the last loaded data while another key loads or after it failed (`data` of `loading` / `failed`). */
  keepData?: boolean;
  /** Wait this long after the key settles before fetching (debounce). */
  delayMs?: number;
}

interface Settled<T> {
  request: string;
  data: T | null;
  error: AppError | null;
}

/**
 * Fetches whenever `key` changes; `null` fetches nothing and stays `loading`. The key must name every input
 * of `fetcher`. A reply for a superseded key, or after unmount, is dropped. `reload` fetches the key again.
 */
export function useApiResult<T>(key: string | null, fetcher: () => Promise<T>, options: ApiResultOptions = {}): ApiResult<T> & { reload: () => void } {
  const { onError, keepData = false, delayMs = 0 } = options;
  const fetcherRef = useRef(fetcher);
  fetcherRef.current = fetcher;
  const onErrorRef = useRef(onError);
  onErrorRef.current = onError;
  const [generation, setGeneration] = useState(0);
  const [settled, setSettled] = useState<Settled<T> | null>(null);
  const request = key === null ? null : `${generation}\u0000${key}`;

  useEffect(() => {
    if (request === null) return;
    let alive = true;
    const run = () =>
      fetcherRef.current().then(
        (data) => alive && setSettled({ request, data, error: null }),
        (e: unknown) => {
          if (!alive) return;
          const error = toAppError(e);
          setSettled((prev) => ({ request, data: prev?.data ?? null, error }));
          onErrorRef.current?.(error);
        },
      );
    const timer = delayMs > 0 ? setTimeout(() => void run(), delayMs) : undefined;
    if (timer === undefined) void run();
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [request, delayMs]);

  const reload = useCallback(() => setGeneration((g) => g + 1), []);
  const kept = keepData ? (settled?.data ?? null) : null;
  if (request === null || settled?.request !== request) return { state: "loading", data: kept, error: null, reload };
  if (settled.error) return { state: "failed", data: kept, error: settled.error, reload };
  return { state: "loaded", data: settled.data as T, error: null, reload };
}
