import { PanelLeft } from "lucide-react";
import { api } from "../api";
import { useBindingLabel } from "../commands";
import { formatAddress } from "../lib/address";
import { copyText } from "../lib/clipboard";
import { formatCount } from "../lib/format";
import { isWpView, selectActivePending, selectActiveTab, selectSidebarHidden, selectWp, useStore, type QueryTab, type Tab } from "../state/store";

/** WordPress mode: the site URL, the table prefix and WooCommerce's stored version and HPOS mode. */
function WpFacts() {
  const wp = useStore(selectWp);
  if (!wp) return null;
  const wc = wp.context.wc;
  const url = wp.context.site.url;
  return (
    <>
      {url && (
        <button data-stage="statusbar:site" className="status-text" title={`Open ${url}`} onClick={() => void api.openUrl(url)}>
          {url.replace(/^https?:\/\//, "")}
        </button>
      )}
      <span>prefix {wp.context.prefix}</span>
      {wc && <span>WooCommerce {wc.version ?? "?"} · hpos {wc.hpos}</span>}
    </>
  );
}

/** The tab's address as text; click copies it. A typed SQL filter is not part of it and shows as a hint. */
function AddressText({ tab }: { tab: Tab }) {
  if (!tab.address) return null;
  const text = formatAddress(tab.address);
  const filtered = tab.kind === "table" && !!tab.filter;
  return (
    <>
      <button data-stage="statusbar:address" className="status-text address mono" title="Copy address" onClick={() => void copyText(text, "address")}>
        {text}
      </button>
      {filtered && <span className="o3" title={`WHERE ${tab.filter}`}>· filtered</span>}
    </>
  );
}

function QuerySummary({ tab }: { tab: QueryTab }) {
  const rowLimit = useStore((s) => s.settings?.rowLimit ?? 0);
  if (!tab.response || tab.error) return null;
  const r = tab.response.results[tab.activeResult];
  return (
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
}

/** The address slot: where the active tab is, and a query tab's result summary. */
function Address() {
  const tab = useStore(selectActiveTab);
  const wpMode = useStore(isWpView);
  if (wpMode) return <WpFacts />;
  if (!tab) return null;
  return (
    <>
      <AddressText tab={tab} />
      {tab.kind === "query" && <QuerySummary tab={tab} />}
    </>
  );
}

function SidebarToggle() {
  const hidden = useStore(selectSidebarHidden);
  const keys = useBindingLabel("view.sidebar");
  return (
    <button
      data-stage="statusbar:sidebar"
      className="icon-btn status-icon"
      aria-label="Toggle sidebar"
      aria-pressed={!hidden}
      title={keys ? `Toggle sidebar (${keys})` : "Toggle sidebar"}
      onClick={() => useStore.getState().toggleSidebar()}
    >
      <PanelLeft size="1.077rem" />
    </button>
  );
}

function PendingCount() {
  const pending = useStore((s) => selectActivePending(s).length);
  const applyKey = useBindingLabel("changes.apply");
  if (pending === 0) return <span>pending 0</span>;
  return (
    <button
      data-stage="statusbar:pending"
      className="status-text pending"
      aria-label={`${pending} pending ${pending === 1 ? "change" : "changes"}`}
      title={`Review and apply in the tray${applyKey ? ` (${applyKey})` : ""}`}
      onClick={() => useStore.getState().focusPendingTray()}
    >
      pending {pending}
    </button>
  );
}

/** Left to right: sidebar toggle, address, spacer, pending count. */
export function StatusBar() {
  const wpMode = useStore(isWpView);
  return (
    <footer className="statusbar" data-stage="statusbar" data-stage-label="Status bar">
      {!wpMode && <SidebarToggle />}
      <Address />
      <div className="spacer" />
      <PendingCount />
    </footer>
  );
}
