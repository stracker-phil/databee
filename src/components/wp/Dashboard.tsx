import { AlertTriangle, CheckCircle2, Clock, Info, RefreshCw } from "lucide-react";
import { api } from "../../api";
import { formatBytes, formatCount } from "../../lib/format";
import { selectActiveSession, selectWp, useStore } from "../../state/store";
import { ErrorPanel } from "../QueryTab";
import { Segments, StackedBars, Treemap } from "./charts";
import { Ago, Card, Loading, Stat, StatusPill } from "./ui";
import { useWpData } from "./useWpData";

const CATEGORY: Record<string, { label: string; color: string }> = {
  woocommerce: { label: "WooCommerce", color: "var(--tile-green)" },
  action_scheduler: { label: "Action Scheduler", color: "var(--tile-purple)" },
  logs: { label: "Logs", color: "var(--tile-red)" },
  core: { label: "Core", color: "var(--tile-gray)" },
  plugins: { label: "Plugins", color: "var(--tile-blue)" },
};

function compact(n: number) {
  return n >= 1000 ? `${(n / 1000).toFixed(n >= 10_000 ? 0 : 1)}k` : String(n);
}

export function DashboardScreen() {
  const session = useStore(selectActiveSession);
  const wp = useStore(selectWp);
  const { navigate, openEntity, openTableTab, setMode } = useStore.getState();
  const { data, error, loading, reload } = useWpData(api.wpDashboard, []);

  if (error) return <ErrorPanel message={error.message} code={error.code} />;
  if (!data || !wp || !session) return <Loading />;

  const ctx = wp.context;
  const failed = data.actions?.byStatus.find((f) => f.value === "failed")?.count ?? 0;
  const totalRows = data.tables.reduce((s, t) => s + t.rows, 0) || 1;
  const top = data.tables.slice(0, 10);
  const rest = data.tables.slice(10);
  const tiles = [
    ...top.map((t) => ({
      key: t.name,
      label: t.name.replace(ctx.prefix, ""),
      value: Math.max(t.rows, 1),
      sub: `${compact(t.rows)} rows · ${Math.round((t.rows / totalRows) * 100)}%`,
      color: CATEGORY[t.category]?.color ?? "var(--tile-gray)",
    })),
    ...(rest.length
      ? [{ key: "__rest", label: `+${rest.length}`, value: Math.max(1, rest.reduce((s, t) => s + t.rows, 0)), sub: `${compact(rest.reduce((s, t) => s + t.rows, 0))} rows`, color: "var(--tile-gray)" }]
      : []),
  ];
  const syncLabel =
    ctx.wc?.hpos === "on_with_sync"
      ? data.hposDrift
        ? `HPOS sync: ${data.hposDrift} differ`
        : "HPOS in sync"
      : ctx.wc?.hpos === "on"
        ? "HPOS"
        : ctx.wc
          ? "Legacy orders"
          : null;

  return (
    <div className="dash">
      <div className="dash-head">
        <span className="dot" />
        <h2>{session.displayName}</h2>
        <span className="muted">{session.serverVersion.split("-")[0]} · {ctx.prefix}</span>
        {ctx.wc?.version && <span className="wp-badge">WooCommerce {ctx.wc.version}</span>}
        {syncLabel && (
          <span className={`wp-badge ${data.hposDrift ? "tone-red" : "tone-green"}`}>
            {data.hposDrift ? <AlertTriangle size="0.846rem" /> : <CheckCircle2 size="0.846rem" />} {syncLabel}
          </span>
        )}
        <div className="spacer" />
        <button className="icon-btn" aria-label="Refresh dashboard" onClick={reload}>
          <RefreshCw size="1rem" className={loading ? "spin" : undefined} />
        </button>
      </div>

      <div className="stat-grid">
        <Stat label="Orders" value={formatCount(data.orders?.total ?? 0)} sub={data.orders ? `+${data.orders.today} today` : "no WooCommerce"} tone={data.orders?.today ? "green" : undefined} />
        <Stat label="Autoload" value={formatBytes(data.autoloadBytes)} sub={data.autoloadHealthy ? "healthy < 800 KB" : "heavy, check plugins"} tone={data.autoloadHealthy ? undefined : "orange"} />
        <Stat label="Failed actions" value={formatCount(failed)} sub={failed ? "needs a look" : "all good"} tone={failed ? "red" : "green"} />
        <Stat label="Expired transients" value={formatCount(data.transientsExpired)} sub={data.transientsExpired ? "cleanup ready" : "none"} tone={data.transientsExpired ? "orange" : undefined} />
      </div>

      {data.insights.length > 0 && (
        <div className="insights">
          {data.insights.map((i) => (
            <div key={i.text} className={`insight ${i.level}`}>
              {i.level === "info" ? <Info size="1rem" /> : <AlertTriangle size="1rem" />} {i.text}
            </div>
          ))}
        </div>
      )}

      <Card
        title="Table sizes by rows · click a tile"
        aside={
          <span className="legend">
            {Object.entries(CATEGORY).map(([k, c]) => (
              <span key={k} className="legend-item"><i style={{ background: c.color }} /> {c.label}</span>
            ))}
          </span>
        }
      >
        <Treemap
          items={tiles}
          onSelect={(key) => {
            if (key === "__rest" || !session.database) return;
            setMode("db");
            openTableTab(session.database, key);
          }}
        />
      </Card>

      <div className="dash-row">
        {data.orders && (
          <Card title="Orders · last 14 days" aside="hover a day" className="grow">
            <StackedBars days={data.orders.days} />
          </Card>
        )}
        {data.actions && (
          <Card title="Action Scheduler" aside={`${formatCount(data.actions.total)} actions`} className="grow">
            <Segments
              parts={[
                { label: "complete", value: data.actions.byStatus.find((f) => f.value === "complete")?.count ?? 0, color: "var(--chart-green)" },
                { label: "pending", value: data.actions.byStatus.find((f) => f.value === "pending")?.count ?? 0, color: "var(--chart-blue)" },
                { label: "failed", value: failed, color: "var(--chart-red)" },
              ]}
            />
            <div className="muted" style={{ margin: "8px 0 10px" }}>
              {data.actions.byStatus.map((f) => `${formatCount(f.count)} ${f.value}`).join(" · ")}
              {data.actions.overdue > 0 && <span className="tone-text-orange"> · {data.actions.overdue} overdue</span>}
            </div>
            <div className="mini-list">
              {data.actions.recentFailed.map((a) => (
                <button key={a.id} className="mini-row" onClick={() => openEntity("action", a.id)} title={a.message ?? undefined}>
                  <AlertTriangle size="1rem" className="tone-text-red" />
                  <span className="mono grow-text">{a.hook}</span>
                  <span className="muted"><Ago unix={a.lastAttempt ?? a.scheduled} /></span>
                </button>
              ))}
              {data.actions.recentFailed.length === 0 && (
                <div className="mini-row muted"><Clock size="1rem" /> No failed actions</div>
              )}
            </div>
            <button className="link-btn" style={{ marginTop: 8 }} onClick={() => navigate({ screen: "list", list: "actions", params: { status: "failed" } })}>
              All failed actions →
            </button>
          </Card>
        )}
      </div>

      {data.recentOrders.length > 0 && (
        <Card title="Recent orders" aside="click opens the order">
          <div className="rows">
            {data.recentOrders.map((o) => (
              <button key={o.id} className="row-link" onClick={() => openEntity("order", o.id)}>
                <span className="row-id">#{o.id}</span>
                <span className="grow-text pii">{o.name || o.email || "guest"}</span>
                <StatusPill status={o.status} />
                <span className="row-gateway">{o.gatewayTitle ?? o.gateway ?? ""}</span>
                <span className="money row-total">{o.total}</span>
                <span className="muted row-when"><Ago unix={o.created} /></span>
              </button>
            ))}
          </div>
        </Card>
      )}
    </div>
  );
}
