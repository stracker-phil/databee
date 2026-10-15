import { formatCount } from "../lib/format";
import { selectActiveSession, selectActiveTab, selectWp, useStore } from "../state/store";

export function StatusBar() {
  const session = useStore(selectActiveSession);
  const tab = useStore(selectActiveTab);
  const rowLimit = useStore((s) => s.settings?.rowLimit ?? 0);
  const wp = useStore(selectWp);
  const wpMode = useStore((s) => s.mode === "wp");
  const pending = useStore((s) => s.pending.length);

  let left: React.ReactNode = null;
  if (wpMode && wp) {
    const wc = wp.context.wc;
    left = (
      <>
        {wp.context.site.url && <span>{wp.context.site.url.replace(/^https?:\/\//, "")}</span>}
        <span>prefix {wp.context.prefix}</span>
        {wc && <span>WooCommerce {wc.version ?? "?"}{wc.hpos === "off" ? " · legacy orders" : wc.hpos === "on_with_sync" ? " · HPOS + sync" : " · HPOS"}</span>}
      </>
    );
  } else if (tab?.kind === "query" && tab.response && !tab.error) {
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
      {pending > 0 && <span className="warn">{pending} pending {pending === 1 ? "change" : "changes"} · ⌘S applies</span>}
      {session && <span title={session.identity}>{session.serverVersion}</span>}
    </footer>
  );
}
