import { useEffect } from "react";
import { PanelLeft } from "lucide-react";
import { useBindingLabel } from "../commands";
import { formatAddress } from "../lib/address";
import { copyText } from "../lib/clipboard";
import { provenanceText } from "../lib/provenance";
import { projectKey } from "../state/slices/project";
import { selectActiveLive, selectActivePending, selectActiveSession, selectActiveTab, selectFocusedValue, selectSidebarHidden, useStore, type Tab } from "../state/store";
import { tabView } from "./tabViews";

/** The tab's address as text; click copies it. A typed SQL filter is not part of it and shows as a hint. */
function AddressText({ tab }: { tab: Tab }) {
  if (!tab.address) return null;
  const text = formatAddress(tab.address);
  return (
    <>
      <button data-stage="statusbar:address" className="status-text address mono" title="Copy address" onClick={() => void copyText(text, "address")}>
        {text}
      </button>
      {tabView(tab).addressHint?.(tab)}
    </>
  );
}

/** Where the hovered value is stored (or a hovered band's rule), else the focused value's (docs/ux.md, Value grammar). */
function useProvenance(): string | null {
  const hovered = useStore((s) => s.hover.src);
  const focused = useStore(selectFocusedValue);
  if (hovered !== null) return hovered;
  return focused ? provenanceText(focused) : null;
}

/** The address slot: the provenance of the hovered or focused value, else where the active tab is, and the tab's summary (a query's results). */
function Address() {
  const tab = useStore(selectActiveTab);
  const provenance = useProvenance();
  const place = provenance ? <span className="address mono">{provenance}</span> : tab && <AddressText tab={tab} />;
  const Summary = tab && tabView(tab).Summary;
  return (
    <>
      {place}
      {tab && Summary && <Summary tab={tab} />}
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
  const sessionId = session?.id;
  useEffect(() => {
    if (sessionId && database) void useStore.getState().loadRulesInfo(sessionId, database);
  }, [sessionId, database, dataVersion]);
  if (!session || !database || !info) return null;
  const presets = info.appliedPresets.length ? info.appliedPresets.join(" · ") : "none";
  return (
    <button data-stage="statusbar:presets" className="status-text" title="Open the Project page" onClick={() => useStore.getState().openProject()}>
      presets {presets}
    </button>
  );
}

/** `● live 2 s` in the accent while the active object column is live; a click ends live mode. */
function LiveText() {
  const seconds = useStore(selectActiveLive);
  const keys = useBindingLabel("object.live");
  if (seconds === null) return null;
  return (
    <button data-stage="statusbar:live" className="status-text live" title={keys ? `Stop live mode (${keys})` : "Stop live mode"} onClick={() => void useStore.getState().toggleLive()}>
      ● live {seconds} s
    </button>
  );
}

/** Left to right: sidebar toggle, address, spacer, presets, pending count, live mode. */
export function StatusBar() {
  return (
    <footer className="statusbar" data-stage="statusbar" data-stage-label="Status bar">
      <SidebarToggle />
      <Address />
      <div className="spacer" />
      <PresetsText />
      <PendingCount />
      <LiveText />
    </footer>
  );
}
