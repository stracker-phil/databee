import { formatCount } from "../lib/format";
import { selectActiveSession, selectActiveTab, useStore } from "../state/store";

export function StatusBar() {
  const session = useStore(selectActiveSession);
  const tab = useStore(selectActiveTab);
  const rowLimit = useStore((s) => s.settings?.rowLimit ?? 0);

  let left: React.ReactNode = null;
  if (tab?.kind === "query" && tab.response && !tab.error) {
    const r = tab.response.results[tab.activeResult];
    left = (
      <>
        {r?.returnsRows && (
          <span className={r.truncated ? "warn" : undefined}>
            {r.truncated
              ? `Showing ${formatCount(r.rows.length)} of ${formatCount(r.totalRows)} rows (row limit ${formatCount(rowLimit)})`
              : `${formatCount(r.rows.length)} ${r.rows.length === 1 ? "row" : "rows"}`}
          </span>
        )}
        <span>{formatCount(tab.response.elapsedMs)} ms</span>
        {tab.response.results.length > 1 && <span>{tab.response.results.length} statements</span>}
      </>
    );
  } else if (tab?.kind === "table") {
    left = <span>{tab.database}.{tab.table}</span>;
  }

  return (
    <footer className="statusbar">
      {left}
      <div className="spacer" />
      {session && <span title={session.identity}>{session.serverVersion}</span>}
    </footer>
  );
}
