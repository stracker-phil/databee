import { useCallback, useEffect, useRef, useState } from "react";
import { api, toAppError, type BrowsePage } from "../api";
import { changeCounts, changesText, freshCheck, rowsToRead, TABLE_CHECK_MS, withHashes, withRows, type TableCheckState } from "../lib/tableCheck";
import { useStore, type TableTab } from "../state/store";

/** Whether the window has the focus and is visible; checks wait while it is in the background. */
function useWindowFocus(): boolean {
  const [focused, setFocused] = useState(() => document.hasFocus() && document.visibilityState === "visible");
  useEffect(() => {
    const update = () => setFocused(document.hasFocus() && document.visibilityState === "visible");
    window.addEventListener("focus", update);
    window.addEventListener("blur", update);
    document.addEventListener("visibilitychange", update);
    return () => {
      window.removeEventListener("focus", update);
      window.removeEventListener("blur", update);
      document.removeEventListener("visibilitychange", update);
    };
  }, []);
  return focused;
}

/** A pending plan that writes the tab's table, or a rule action's plan, which may write any table. */
function usePendingForTable(tab: TableTab): boolean {
  return useStore((s) => s.pending.some((p) => p.sessionId === tab.sessionId && (!p.target || (p.target.database === tab.database && p.target.table === tab.table))));
}

export type TableCheckStatus = { kind: "off" } | { kind: "unavailable"; reason: string } | { kind: "paused"; reason: string } | { kind: "failed"; message: string } | { kind: "on" };

interface Options {
  /** The table has a primary key (the page carries row keys only when it can read them as text). */
  hasPrimaryKey: boolean;
  /** The grid's inline editor is open. */
  editing: boolean;
  /** Another page is on its way. */
  loading: boolean;
}

/**
 * The live check of a table tab's page: right after the page loads its rows' checksums are the
 * baseline, then every `TABLE_CHECK_MS` while the tab is active, the window has the focus, no cell is
 * being edited and no change to the table is pending, the rows are read again by key (one statement;
 * one more for rows whose stored values are not read yet). It never changes what the page shows.
 */
export function useTableCheck(tab: TableTab, page: BrowsePage | null, { hasPrimaryKey, editing, loading }: Options) {
  const enabled = useStore((s) => s.settings?.tableLiveCheck ?? false);
  const active = useStore((s) => s.activeTabId === tab.id);
  const pending = usePendingForTable(tab);
  const focused = useWindowFocus();
  const [check, setCheckState] = useState<TableCheckState | null>(null);
  const current = useRef<TableCheckState | null>(null);
  const setCheck = useCallback((next: TableCheckState | null) => {
    current.current = next;
    setCheckState(next);
  }, []);
  const [round, setRound] = useState(0);

  const rows = page?.result.rows.length ?? 0;
  const keyed = !!page && rows > 0 && page.keys.length === rows;
  const resultId = page?.result.id ?? null;
  const usable = enabled && keyed;

  // Each check reads the page it was started for; a reply for an earlier page is dropped.
  const runCheck = async (page: BrowsePage) => {
    const columns = page.result.columns.map((c) => c.name);
    const target = { database: tab.database, table: tab.table, columns };
    const isCurrent = () => current.current?.resultId === page.result.id;
    try {
      const { hashes } = await api.tableCheck(tab.sessionId, { ...target, keys: page.keys });
      if (!isCurrent() || !current.current) return;
      const next = withHashes(current.current, hashes, Date.now());
      setCheck(next);
      const read = rowsToRead(next, hashes);
      if (read.length === 0) return;
      const shown = read.map((row) => ({ row, key: page.keys[row], cells: page.result.rows[row] }));
      const checked = await api.tableCheckRows(tab.sessionId, { ...target, resultId: page.result.id, rows: shown });
      if (isCurrent() && current.current) setCheck(withRows(current.current, checked));
    } catch (e) {
      if (isCurrent() && current.current) setCheck({ ...current.current, error: toAppError(e).message });
    }
  };

  // The timers run the latest render's check on the latest page, whose identity is its result id.
  const latest = useRef({ page, runCheck });
  useEffect(() => {
    latest.current = { page, runCheck };
  });
  const runLatest = useCallback(() => {
    const { page, runCheck } = latest.current;
    if (page) void runCheck(page).finally(() => setRound((r) => r + 1));
  }, []);

  // A new page (Refresh, paging, a filter, ⌘R) starts over; its baseline is read right away.
  useEffect(() => {
    setCheck(resultId !== null && usable ? freshCheck(resultId) : null);
    if (resultId !== null && usable) runLatest();
  }, [resultId, usable, setCheck, runLatest]);

  const pausedBy = pending ? "pending changes" : editing ? "editing" : null;
  const ticking = usable && active && focused && !loading && !pausedBy && !!check?.baseline && !check.error;
  useEffect(() => {
    if (!ticking) return;
    const timer = setTimeout(runLatest, TABLE_CHECK_MS);
    return () => clearTimeout(timer);
  }, [ticking, round, runLatest]);

  const status: TableCheckStatus = !enabled
    ? { kind: "off" }
    : !page || rows === 0
      ? { kind: "off" }
      : !keyed
        ? { kind: "unavailable", reason: hasPrimaryKey ? "the primary key is not readable as text" : "no primary key" }
        : check?.error
          ? { kind: "failed", message: check.error }
          : pausedBy === "pending changes"
            ? { kind: "paused", reason: pausedBy }
            : { kind: "on" };
  return { status, rows: check?.rows };
}

/** Why the tab does not check its rows, in the table head; nothing while it checks. */
export function TableCheckHint({ status }: { status: TableCheckStatus }) {
  if (status.kind === "unavailable") return <span className="hint" data-stage="table:live-check" title="The live check reads the shown rows again by primary key">no live check · {status.reason}</span>;
  if (status.kind === "paused") return <span className="hint" data-stage="table:live-check" title="Apply or discard them to check the rows again">live check paused · {status.reason}</span>;
  if (status.kind === "failed") return <span className="hint" data-stage="table:live-check" title={status.message}>live check stopped · Refresh to retry</span>;
  return null;
}

/** `3 rows changed in the database · Refresh` above the grid; nothing while every row is as shown. */
export function TableChangesBar({ rows, onRefresh }: { rows: TableCheckState["rows"] | undefined; onRefresh: () => void }) {
  const counts = changeCounts(rows);
  const text = changesText(counts);
  if (!text) return null;
  return (
    <div className="db-changes" role="status" data-stage="table:changes" data-stage-label="Changed in the database">
      <span>{text}</span>
      <span className="o3">·</span>
      <button className="link-btn plain" onClick={onRefresh}>
        Refresh
      </button>
      {counts.changed > 0 && <span className="o3 db-changes-hint">⌥ on a marked value shows old → new</span>}
    </div>
  );
}
