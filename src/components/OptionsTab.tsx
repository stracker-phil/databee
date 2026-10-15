import { useEffect, useState } from "react";
import { Eraser, RefreshCw } from "lucide-react";
import { api, toAppError, type AppError, type OptionsOverview } from "../api";
import { formatBytes, formatCount } from "../lib/format";
import { useStore, type OptionsTab as OptionsTabState } from "../state/store";
import { ErrorPanel } from "./QueryTab";

function expiry(expiresAt: number | null, expired: boolean): string {
  if (expiresAt == null) return "never expires";
  const date = new Date(expiresAt * 1000).toLocaleString();
  return expired ? `expired ${date}` : `expires ${date}`;
}

export function OptionsTab({ tab }: { tab: OptionsTabState }) {
  const { openTableTab, openQueryTab, toast } = useStore.getState();
  const [data, setData] = useState<OptionsOverview | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [version, setVersion] = useState(0);

  useEffect(() => {
    api.wpOptions(tab.sessionId, tab.database).then(
      (d) => {
        setData(d);
        setError(null);
      },
      (e) => setError(toAppError(e)),
    );
  }, [tab.sessionId, tab.database, version]);

  if (error) return <ErrorPanel message={error.message} code={error.code} />;
  if (!data) return <div className="empty-state">Loading…</div>;

  const expired = data.transients.filter((t) => t.expired);
  const maxAutoload = Math.max(1, ...data.groups.map((g) => g.autoloadBytes));
  const showGroup = (prefix: string) => {
    const like =
      prefix === "transients"
        ? "option_name LIKE '\\_transient\\_%' OR option_name LIKE '\\_site\\_transient\\_%'"
        : prefix === "(single)"
          ? "option_name NOT LIKE '%\\_%' AND option_name NOT LIKE '%-%'"
          : `option_name LIKE '${prefix}%' OR option_name LIKE '\\_${prefix}%'`;
    openTableTab(tab.database, data.table, "data", like);
  };

  return (
    <div className="doc">
      <div className="doc-head">
        <h2>Options</h2>
        <span className="muted">{data.table}</span>
        <div className="spacer" />
        <button className="icon-btn" aria-label="Reload options" onClick={() => setVersion((v) => v + 1)}>
          <RefreshCw size={13} />
        </button>
      </div>
      <div className="stat-row">
        <div className="stat"><div className="n">{formatCount(data.totalCount)}</div><div className="l">options</div></div>
        <div className="stat"><div className="n">{formatBytes(data.autoloadBytes)}</div><div className="l">autoloaded on every request</div></div>
        <div className="stat"><div className="n">{formatCount(data.transients.length)}</div><div className="l">transients</div></div>
        <div className="stat"><div className="n" style={{ color: expired.length ? "var(--warning)" : undefined }}>{formatCount(expired.length)}</div><div className="l">expired</div></div>
      </div>

      <section className="doc-section structure" style={{ padding: 0 }}>
        <h3>By prefix</h3>
        <table>
          <thead>
            <tr><th>Prefix</th><th>Options</th><th>Size</th><th>Autoloaded</th><th style={{ width: "30%" }} /></tr>
          </thead>
          <tbody>
            {data.groups.map((g) => (
              <tr key={g.prefix} onClick={() => showGroup(g.prefix)} style={{ cursor: "default" }} title="Show these options">
                <td style={{ fontWeight: 600 }}>{g.prefix}</td>
                <td>{formatCount(g.count)}</td>
                <td>{formatBytes(g.bytes)}</td>
                <td>{g.autoloadCount ? `${formatCount(g.autoloadCount)} · ${formatBytes(g.autoloadBytes)}` : "—"}</td>
                <td><div className="bar" style={{ width: `${(g.autoloadBytes / maxAutoload) * 100}%` }} /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <section className="doc-section structure" style={{ padding: 0 }}>
        <h3 style={{ display: "flex", alignItems: "center" }}>
          Transients
          <div className="spacer" />
          <button
            className="btn"
            disabled={expired.length === 0}
            onClick={async () => {
              try {
                openQueryTab(await api.wpTransientCleanupSql(tab.sessionId, tab.database));
              } catch (e) {
                toast("error", toAppError(e).message);
              }
            }}
          >
            <Eraser size={13} /> Cleanup SQL for {expired.length} expired
          </button>
        </h3>
        {data.transients.length === 0 ? (
          <div className="muted">No transients stored in the database.</div>
        ) : (
          <table>
            <thead>
              <tr><th>Name</th><th>Size</th><th>Expiry</th></tr>
            </thead>
            <tbody>
              {data.transients.map((t) => (
                <tr key={`${t.siteWide}-${t.name}`}>
                  <td className="mono">{t.name}{t.siteWide && <span className="badge" style={{ marginLeft: 6 }}>site</span>}</td>
                  <td>{formatBytes(t.bytes)}</td>
                  <td style={{ color: t.expired ? "var(--warning)" : undefined }}>{expiry(t.expiresAt, t.expired)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </div>
  );
}
