import { useCallback, useEffect, useRef, useState } from "react";
import { api, toAppError, type BrowsePage } from "../api";
import { changeCounts, changesText, checkStatus, freshCheck, rowsToRead, TABLE_CHECK_MS, withHashes, withRows, type PauseReason, type TableCheckState, type TableCheckStatus } from "../lib/tableCheck";
import { useStore, type TableTab } from "../state/store";

const isWindowFocused = () => document.hasFocus() && document.visibilityState === "visible";

/** Whether the window has the focus and is visible; checks wait while it is in the background. */
function useWindowFocus(): boolean {
  const [focused, setFocused] = useState(isWindowFocused);
  useEffect(() => {
    const update = () => setFocused(isWindowFocused());
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

/** Reads the checksums of `page`'s rows, then the changed rows not read yet; a reply for an earlier page is dropped. */
async function checkPage(tab: TableTab, page: BrowsePage, latestCheck: () => TableCheckState | null, setCheck: (next: TableCheckState) => void) {
  const columns = page.result.columns.map((c) => c.name);
  const target = { database: tab.database, table: tab.table, columns };
  const checkForPage = () => {
    const check = latestCheck();
    return check?.resultId === page.result.id ? check : null;
  };
  try {
    const { hashes } = await api.tableCheck(tab.sessionId, { ...target, keys: page.keys });
    const hashed = checkForPage();
    if (!hashed) return;
    const next = withHashes(hashed, hashes, Date.now());
    setCheck(next);
    const read = rowsToRead(next, hashes);
    if (read.length === 0) return;
    const shown = read.map((row) => ({ row, key: page.keys[row], cells: page.result.rows[row] }));
    const checked = await api.tableCheckRows(tab.sessionId, { ...target, resultId: page.result.id, rows: shown });
    const stored = checkForPage();
    if (stored) setCheck(withRows(stored, checked));
  } catch (e) {
    const failed = checkForPage();
    if (failed) setCheck({ ...failed, error: toAppError(e).message });
  }
}

interface Options {
  /** The table has a primary key (the page carries row keys only when it can read them as text). */
  hasPrimaryKey: boolean;
  editing: boolean;
  loading: boolean;
}

/** The live check of a table tab's page, every `TABLE_CHECK_MS` against the checksums read right after it loaded. */
export function useTableCheck(tab: TableTab, page: BrowsePage | null, { hasPrimaryKey, editing, loading }: Options) {
  const enabled = useStore((s) => s.settings?.tableLiveCheck ?? false);
  const active = useStore((s) => s.activeTabId === tab.id);
  const pending = usePendingForTable(tab);
  const lost = useStore((s) => !!s.health[tab.sessionId]);
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

  // The timers check the latest page with the latest tab.
  const latest = useRef({ tab, page });
  useEffect(() => {
    latest.current = { tab, page };
  });
  const runLatest = useCallback(() => {
    const now = latest.current;
    if (now.page) void checkPage(now.tab, now.page, () => current.current, setCheck).finally(() => setRound((r) => r + 1));
  }, [setCheck]);

  // Refresh, paging, a filter or ⌘R start over.
  useEffect(() => {
    setCheck(resultId !== null && usable ? freshCheck(resultId) : null);
    if (resultId !== null && usable) runLatest();
  }, [resultId, usable, setCheck, runLatest]);

  const pausedBy: PauseReason | null = lost ? "connection lost" : pending ? "pending changes" : editing ? "editing" : null;
  const ticking = usable && active && focused && !loading && !pausedBy && !!check?.baseline && !check.error;
  useEffect(() => {
    if (!ticking) return;
    const timer = setTimeout(runLatest, TABLE_CHECK_MS);
    return () => clearTimeout(timer);
  }, [ticking, round, runLatest]);

  const status = checkStatus({ enabled, rows, keyed, hasPrimaryKey, error: check?.error, pausedBy });
  return { status, rows: check?.rows };
}

/** Why the tab does not check its rows, in the table head; nothing while it checks. */
export function TableCheckHint({ status }: { status: TableCheckStatus }) {
  if (status.kind === "unavailable") return <span className="hint" data-stage="table:live-check" title="The live check reads the shown rows again by primary key">no live check · {status.reason}</span>;
  if (status.kind === "paused") {
    const title = status.reason === "connection lost" ? "Checks again once the connection is back" : "Apply or discard them to check the rows again";
    return <span className="hint" data-stage="table:live-check" title={title}>live check paused · {status.reason}</span>;
  }
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
