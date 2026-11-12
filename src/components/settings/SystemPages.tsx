/** Settings → Status (the build, the log level in effect, the open connections) and About (version, folders, license). */
import { useId } from "react";
import { api, LOG_LEVELS, type AppStatus, type ConnectionStatus, type LogLevel } from "../../api";
import { useApiResult } from "../../lib/useApiResult";
import { PopUpButton } from "../PopUpButton";
import type { SettingsPageProps, SettingsRow } from "../../registry/settingsPages";
import { capitalize, Group, Row } from "./controls";

const SOURCE_URL = "https://github.com/stracker-phil/databee";

export const STATUS_ROWS: SettingsRow[] = [
  { id: "status-build", title: "Build" },
  { id: "status-version", title: "Version" },
  { id: "status-kind", title: "Kind", keywords: ["debug", "release"] },
  { id: "status-log", title: "Log" },
  { id: "status-log-level", title: "Log level", keywords: ["logging", "trace", "debug"] },
  { id: "status-connections", title: "Open connections", keywords: ["sessions", "health", "server"] },
];

export const ABOUT_ROWS: SettingsRow[] = [
  { id: "about-version", title: "Version" },
  { id: "about-data-folder", title: "Data folder", keywords: ["settings", "config", "connections", "projects", "history"] },
  { id: "about-log-folder", title: "Log folder", keywords: ["logs", "logging"] },
  { id: "about-license", title: "License" },
  { id: "about-source", title: "Source code", keywords: ["github"] },
];

function useStatus() {
  return useApiResult("app-status", api.appStatus);
}

function healthText(connection: ConnectionStatus): string {
  switch (connection.health.state) {
    case "ok":
      return "Connected";
    case "down":
      return `Down: ${connection.health.error}`;
    default:
      return "Reconnecting…";
  }
}

function LogLevelRow({ s, set, status }: SettingsPageProps & { status: AppStatus | null }) {
  const id = useId();
  const fromCli = status?.logLevelFromCli ? ` This run uses ${status.logLevel} from --log-level.` : "";
  const detail = s.logLevel === "trace" ? "Trace logs include the SQL text of every query." : "Higher levels write more detail.";
  return (
    <Row label="Log level" htmlFor={id} anchor="status-log-level" description={detail + fromCli}>
      <PopUpButton<LogLevel>
        id={id}
        className="input"
        value={s.logLevel}
        options={LOG_LEVELS.map((level) => ({ value: level, label: capitalize(level) }))}
        onChange={(logLevel) => set({ logLevel })}
      />
    </Row>
  );
}

export function StatusPage({ s, set }: SettingsPageProps) {
  const { data: status, reload } = useStatus();
  const connections = status?.connections ?? [];
  return (
    <>
      <Group title="Build" anchor="status-build">
        <Row label="Version" anchor="status-version">
          <span className="settings-value mono">{status?.version ?? ""}</span>
        </Row>
        <Row label="Kind" anchor="status-kind" description={status?.build === "debug" ? "The separate debug app, on demo data." : undefined}>
          <span className="settings-value">{status ? capitalize(status.build) : ""}</span>
        </Row>
      </Group>
      <Group title="Log" anchor="status-log">
        <LogLevelRow s={s} set={set} status={status} />
      </Group>
      <Group title="Open connections" anchor="status-connections" list>
        {connections.length === 0 && <div className="settings-row settings-empty">No open connections.</div>}
        {connections.map((c) => (
          <Row key={`${c.slug}:${c.database ?? ""}`} label={c.database ? `${c.slug} · ${c.database}` : c.slug} description={`${c.account} · ${c.serverVersion}`}>
            <span className={`settings-value${c.health.state === "ok" ? "" : " danger"}`}>{healthText(c)}</span>
          </Row>
        ))}
        <Row label="Refresh">
          <button type="button" className="btn" onClick={reload}>Refresh</button>
        </Row>
      </Group>
    </>
  );
}

export function AboutPage() {
  const { data: status } = useStatus();
  return (
    <Group>
      <Row label="Version" anchor="about-version">
        <span className="settings-value mono">{status?.version ?? ""}</span>
      </Row>
      <Row label="Data folder" anchor="about-data-folder" description={status?.dataFolder}>
        <button type="button" className="btn" onClick={() => void api.runCommand("help.showDataFolder")}>Show Data Folder</button>
      </Row>
      <Row label="Log folder" anchor="about-log-folder" description={status?.logFolder}>
        <button type="button" className="btn" onClick={() => void api.runCommand("help.openLogFolder")}>Open Log Folder</button>
      </Row>
      <Row label="License" anchor="about-license">
        <span className="settings-value">MIT</span>
      </Row>
      <Row label="Source code" anchor="about-source" description={SOURCE_URL}>
        <button type="button" className="btn" onClick={() => void api.openUrl(SOURCE_URL)}>Open in Browser</button>
      </Row>
    </Group>
  );
}
