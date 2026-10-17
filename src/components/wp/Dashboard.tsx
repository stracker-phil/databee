import { AlertTriangle, Info, RefreshCw } from "lucide-react";
import { api, type Insight } from "../../api";
import { formatBytes, formatCount } from "../../lib/format";
import { selectActiveSession, selectWp, useStore } from "../../state/store";
import { ErrorPanel } from "../ErrorPanel";
import { Card, Loading, Stat } from "./ui";
import { useWpData } from "./useWpData";

/** One alert with its change; writes always go through the pending tray. */
function InsightRow({ insight }: { insight: Insight }) {
  const { queueChange } = useStore.getState();
  const a = insight.action;
  return (
    <div className={`insight ${insight.level}`}>
      {insight.level === "info" ? <Info size="1rem" /> : <AlertTriangle size="1rem" />}
      <span className="grow-text">{insight.text}</span>
      {a && (
        <button className="btn tiny" onClick={() => void queueChange(a.change)}>
          {a.label}
        </button>
      )}
    </div>
  );
}

export function DashboardScreen() {
  const session = useStore(selectActiveSession);
  const wp = useStore(selectWp);
  const { data, error, loading, reload } = useWpData([], api.wpDashboard);

  if (error) return <ErrorPanel message={error.message} code={error.code} />;
  if (!data || !wp || !session) return <Loading />;

  const failed = data.actions?.byStatus.find((f) => f.value === "failed")?.count ?? 0;
  return (
    <div className="dash">
      <div className="dash-head">
        <span className="dot" />
        <h2>{session.displayName}</h2>
        <span className="muted">{session.serverVersion.split("-")[0]} · {wp.prefix}</span>
        {!!data.hposDrift && (
          <span className="wp-badge tone-red">
            <AlertTriangle size="0.846rem" /> {data.hposDrift} wc_orders differ from their posts copy
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

      {data.actions && (
        <Card title="Action Scheduler" aside={`${formatCount(data.actions.total)} actions`}>
          <div className="muted">
            {data.actions.byStatus.map((f) => `${formatCount(f.count)} ${f.value}`).join(" · ")}
            {data.actions.overdue > 0 && <span className="tone-text-orange"> · {data.actions.overdue} overdue</span>}
          </div>
        </Card>
      )}
    </div>
  );
}
