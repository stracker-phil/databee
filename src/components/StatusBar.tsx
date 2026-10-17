import { useEffect } from "react";
import { PanelLeft } from "lucide-react";
import { useBindingLabel } from "../commands";
import { formatAddress } from "../lib/address";
import { copyText } from "../lib/clipboard";
import { formatCount } from "../lib/format";
import { provenanceText } from "../lib/provenance";
import { projectKey } from "../state/slices/project";
import { isWpView, selectActivePending, selectActiveSession, selectActiveTab, selectFocusedValue, selectSidebarHidden, selectWp, useStore, type QueryTab, type Tab } from "../state/store";

/** WordPress mode: the table prefix the presets apply under. */
function WpFacts() {
  const wp = useStore(selectWp);
  if (!wp) return null;
  return <span>prefix {wp.prefix}</span>;
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

/** Where the hovered value is stored (or a hovered band's rule), else the focused value's (docs/ux.md, Value grammar). */
function useProvenance(): string | null {
  const hovered = useStore((s) => s.hover.src);
  const focused = useStore((s) => {
    const value = selectFocusedValue(s);
    return value ? provenanceText(value) : null;
  });
  return hovered ?? focused;
}

/** The address slot: the provenance of the hovered or focused value, else where the active tab is, and a query tab's result summary. */
function Address() {
  const tab = useStore(selectActiveTab);
  const wpMode = useStore(isWpView);
  const provenance = useProvenance();
  const place = provenance ? <span className="address mono">{provenance}</span> : wpMode ? <WpFacts /> : tab && <AddressText tab={tab} />;
  return (
    <>
      {place}
      {!wpMode && tab?.kind === "query" && <QuerySummary tab={tab} />}
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

/** The presets that apply to the active database; a click opens its Project page. */
function PresetsText() {
  const session = useStore(selectActiveSession);
  const database = session?.database ?? null;
  const info = useStore((s) => (session && database ? s.rulesInfo[projectKey(session.id, database)] : undefined));
  const dataVersion = useStore((s) => s.dataVersion);
  useEffect(() => {
    if (session && database) void useStore.getState().loadRulesInfo(session.id, database);
  }, [session?.id, database, dataVersion]);
  if (!session || !database || !info) return null;
  const presets = info.appliedPresets.length ? info.appliedPresets.join(" · ") : "none";
  return (
    <button data-stage="statusbar:presets" className="status-text" title="Open the Project page" onClick={() => useStore.getState().openProject()}>
      presets {presets}
    </button>
  );
}

/** Left to right: sidebar toggle, address, spacer, presets, pending count. */
export function StatusBar() {
  const wpMode = useStore(isWpView);
  return (
    <footer className="statusbar" data-stage="statusbar" data-stage-label="Status bar">
      {!wpMode && <SidebarToggle />}
      <Address />
      <div className="spacer" />
      {!wpMode && <PresetsText />}
      <PendingCount />
    </footer>
  );
}
