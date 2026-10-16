import { Monitor, Moon, PanelBottom, PanelRight, Sun } from "lucide-react";
import { api, type Settings } from "../api";
import { runCommand } from "../commands";
import { formatCount } from "../lib/format";
import { selectActiveSession, selectActiveTab, selectWp, useStore } from "../state/store";

const NEXT_THEME: Record<Settings["theme"], Settings["theme"]> = { system: "light", light: "dark", dark: "system" };
const THEME_ICON = { system: Monitor, light: Sun, dark: Moon };

export function StatusBar() {
  const session = useStore(selectActiveSession);
  const tab = useStore(selectActiveTab);
  const settings = useStore((s) => s.settings);
  const wp = useStore(selectWp);
  const wpMode = useStore((s) => s.mode === "wp");
  const pending = useStore((s) => s.pending.length);
  const { saveSettings, toast } = useStore.getState();
  const rowLimit = settings?.rowLimit ?? 0;

  let left: React.ReactNode = null;
  if (wpMode && wp) {
    const wc = wp.context.wc;
    const url = wp.context.site.url;
    left = (
      <>
        {url && (
          <button data-stage="statusbar:site" className="status-chip" title={`Open ${url}`} onClick={() => void api.openUrl(url)}>
            {url.replace(/^https?:\/\//, "")}
          </button>
        )}
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

  const ThemeIcon = THEME_ICON[settings?.theme ?? "system"];
  const DockIcon = settings?.inspectorDock === "bottom" ? PanelBottom : PanelRight;
  return (
    <footer className="statusbar" data-stage="statusbar" data-stage-label="Status bar">
      {left}
      <div className="spacer" />
      {pending > 0 && (
        <button data-stage="statusbar:pending" className="status-chip warn" title="Review and apply in the tray (⌘S)" onClick={() => document.querySelector<HTMLElement>(".tray .btn-primary")?.focus()}>
          {pending} pending {pending === 1 ? "change" : "changes"}
        </button>
      )}
      {settings && (
        <>
          <button data-stage="statusbar:dock" className="status-chip" aria-label={`Inspector: ${settings.inspectorDock}`} title="Move the inspector (right / bottom)" onClick={() => runCommand("view.inspectorDock")}>
            <DockIcon size="0.923rem" />
          </button>
          <button data-stage="statusbar:theme" className="status-chip" aria-label={`Theme: ${settings.theme}`} title={`Theme: ${settings.theme} (click for ${NEXT_THEME[settings.theme]})`} onClick={() => void saveSettings({ ...settings, theme: NEXT_THEME[settings.theme] }, { quiet: true })}>
            <ThemeIcon size="0.923rem" />
          </button>
        </>
      )}
      {session && (
        <button
          data-stage="statusbar:connection"
          data-stage-label={session.identity}
          className="status-chip"
          title={`${session.serverVersion} · click to copy ${session.identity}`}
          onClick={() => void navigator.clipboard.writeText(session.identity).then(() => toast("success", "Copied connection"))}
        >
          <span className="dot" /> {session.identity}
        </button>
      )}
    </footer>
  );
}
