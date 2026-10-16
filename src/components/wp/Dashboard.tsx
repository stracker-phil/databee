import { useState } from "react";
import { AlertTriangle, CheckCircle2, Clock, Info, RefreshCw } from "lucide-react";
import { api, type HookFailures, type Insight } from "../../api";
import { compactCount, formatBytes, formatCount } from "../../lib/format";
import { selectActiveSession, selectWp, useStore } from "../../state/store";
import { ErrorPanel } from "../QueryTab";
import { BreakdownChart } from "./BreakdownChart";
import { Segments, StackedBars } from "./charts";
import { Ago, Card, Loading, Stat, StatusPill } from "./ui";
import { ChevronDown, ChevronRight } from "lucide-react";
import { useWpData } from "./useWpData";

/** One alert with its action; writes always go through the pending tray. */
function InsightRow({ insight }: { insight: Insight }) {
  const { navigate, queueChange } = useStore.getState();
  const a = insight.action;
  return (
    <div className={`insight ${insight.level}`}>
      {insight.level === "info" ? <Info size="1rem" /> : <AlertTriangle size="1rem" />}
      <span className="grow-text">{insight.text}</span>
      {a && (
        <button
          className="btn tiny"
          onClick={() => {
            if (a.kind === "change" && a.change) void queueChange(a.change);
            else if (a.kind === "navigate" && a.list) navigate({ screen: "list", list: a.list, params: a.status ? { status: a.status } : undefined });
          }}
        >
          {a.label}
        </button>
      )}
    </div>
  );
}

/** A failed hook with its count; expanding lists the individual actions and their last log line. */
function HookGroup({ group }: { group: HookFailures }) {
  const [open, setOpen] = useState(false);
  const openEntity = useStore((s) => s.openEntity);
  const { data } = useWpData([open, group.hook], (s, d) => (open ? api.wpActions(s, d, { status: "failed", kind: group.hook, search: null, offset: 0, limit: 10 }) : Promise.resolve(null)));
  return (
    <div className="hook-group">
      <button className="mini-row" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        {open ? <ChevronDown size="0.846rem" /> : <ChevronRight size="0.846rem" />}
        <span className="mono grow-text">{group.hook}</span>
        <span className="badge">×{formatCount(group.count)}</span>
        <span className="muted">{group.lastFailure ? <>last <Ago unix={group.lastFailure} /></> : ""}</span>
      </button>
      {open && (
        <div className="hook-items">
          {!data && <div className="muted small">Loading…</div>}
          {data?.items.map((a) => (
            <button key={a.id} className="mini-row" onClick={() => openEntity("action", a.id)}>
              <span className="muted">#{a.id}</span>
              <span className="grow-text small">{a.message ?? "no log message"}</span>
              <span className="muted small"><Ago unix={a.lastAttempt ?? a.scheduled} /></span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export function DashboardScreen() {
  const session = useStore(selectActiveSession);
  const wp = useStore(selectWp);
  const { navigate, openEntity, openTableTab, setMode } = useStore.getState();
  const { data, error, loading, reload } = useWpData([], api.wpDashboard);
  const { data: options } = useWpData([], api.wpOptions);
  const [breakdown, setBreakdown] = useState<"autoload" | "rows">("autoload");

  if (error) return <ErrorPanel message={error.message} code={error.code} />;
  if (!data || !wp || !session) return <Loading />;

  const ctx = wp.context;
  const failed = data.actions?.byStatus.find((f) => f.value === "failed")?.count ?? 0;
  const rowItems = data.tables.map((t) => ({
    id: t.name,
    label: t.name.replace(ctx.prefix, ""),
    value: t.rows,
    onOpen: () => {
      if (!session.database) return;
      setMode("db");
      openTableTab(session.database, t.name);
    },
  }));
  const autoloadItems = (options?.groups ?? [])
    .filter((g) => g.autoloadBytes > 0)
    .map((g) => ({
      id: g.prefix,
      label: g.label,
      value: g.autoloadBytes,
      sublabel: `${formatCount(g.autoloadCount)} options`,
      onOpen: () => navigate({ screen: "list", list: "settings", params: { prefix: g.prefix } }),
    }));
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
        <Stat label="Autoload" value={formatBytes(data.autoloadBytes)} sub={data.autoloadHealthy ? "healthy < 800 KB" : "heavy"} tone={data.autoloadHealthy ? undefined : "orange"} />
        <Stat label="Failed actions" value={formatCount(failed)} sub={failed ? "needs a look" : "all good"} tone={failed ? "red" : "green"} />
        <Stat label="Expired transients" value={formatCount(data.transientsExpired)} sub={data.transientsExpired ? "cleanup ready" : "none"} tone={data.transientsExpired ? "orange" : undefined} />
      </div>

      {data.insights.length > 0 && (
        <div className="insights">
          {data.insights.map((i) => (
            <InsightRow key={i.text} insight={i} />
          ))}
        </div>
      )}

      <Card title={breakdown === "autoload" ? "Autoload by option name prefix" : "Table sizes by rows"} aside="click a row to open it">
        <BreakdownChart
          items={breakdown === "autoload" ? autoloadItems : rowItems}
          format={breakdown === "autoload" ? formatBytes : (v) => `${compactCount(v)} rows`}
          totalLabel={breakdown === "autoload" ? "autoloaded" : "rows"}
          modes={[
            { id: "autoload", label: "Autoload" },
            { id: "rows", label: "Table rows" },
          ]}
          mode={breakdown}
          onModeChange={(m) => setBreakdown(m as "autoload" | "rows")}
        />
      </Card>

      <div className="dash-row">
        {data.orders && (
          <Card title={data.orders.granularity === "hour" ? "Orders · last 24 h by hour" : "Orders · last 14 days"} aside={data.orders.granularity === "hour" ? "fewer than 3 active days; hover an hour" : "hover a day"} className="grow">
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
              {data.actions.byHook.map((g) => (
                <HookGroup key={g.hook} group={g} />
              ))}
              {data.actions.byHook.length === 0 && (
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
                <span className="grow-text">{o.name || o.email || "guest"}</span>
                <StatusPill status={o.status} />
                <span className="row-gateway mono" title={o.gatewayTitle ?? undefined}>{o.gateway ?? ""}</span>
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
