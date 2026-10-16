import { useState } from "react";
import { Eraser } from "lucide-react";
import { api } from "../../api";
import { formatBytes, formatCount } from "../../lib/format";
import { selectActiveSession, selectWp, useStore } from "../../state/store";
import { ErrorPanel } from "../ErrorPanel";
import { BreakdownChart } from "./BreakdownChart";
import { FieldList } from "./FieldValue";
import { Ago, Card, Loading, Stat } from "./ui";
import { useWpData } from "./useWpData";

function OptionGroup({ prefix }: { prefix: string }) {
  const session = useStore(selectActiveSession);
  const queueChange = useStore((s) => s.queueChange);
  const wp = useStore(selectWp);
  const { data, error } = useWpData([prefix], (s, d) => api.wpOptionsGroup(s, d, prefix));
  if (error) return <ErrorPanel message={error.message} code={error.code} />;
  if (!data || !session?.database || !wp) return <Loading />;
  return (
    <Card title={`Options: ${prefix}`} aside={`${data.length} options · click a pencil to edit, “autoload” to toggle`}>
      <div className="option-autoload">
        {data.map((f) => {
          const on = f.autoload?.on ?? false;
          return (
            <button
              key={f.name}
              className={`flag-pill ${on ? "on" : "off"}`}
              title={`Queue autoload ${on ? "off" : "on"} for ${f.name}`}
              onClick={() => void queueChange({ type: "setAutoload", database: session.database!, option: f.name, on: !on })}
            >
              {f.name}
            </button>
          );
        })}
      </div>
      <FieldList
        fields={data.map((f) => ({ ...f, label: null }))}
        target={{ kind: "row", table: `${wp.context.prefix}options`, keyColumn: "option_name", key: null, column: "option_value" }}
      />
    </Card>
  );
}

export function SettingsScreen({ params }: { params?: Record<string, string> }) {
  const session = useStore(selectActiveSession);
  const queueChange = useStore((s) => s.queueChange);
  const openRow = useStore((s) => s.openRow);
  const [prefix, setPrefix] = useState<string | null>(params?.prefix ?? null);
  const { data, error } = useWpData([], api.wpOptions);
  if (error) return <ErrorPanel message={error.message} code={error.code} />;
  if (!data || !session?.database) return <Loading />;
  const expired = data.transients.filter((t) => t.expired);
  const owners = data.groups
    .filter((g) => g.autoloadBytes > 0)
    .map((g) => ({
      id: g.prefix,
      label: g.label,
      value: g.autoloadBytes,
      sublabel: `${formatCount(g.autoloadCount)} of ${formatCount(g.count)} options`,
      onOpen: () => setPrefix(g.prefix),
    }));

  return (
    <div className="dash">
      <div className="stat-grid">
        <Stat label="Options" value={formatCount(data.totalCount)} />
        <Stat label="Autoloaded" value={formatBytes(data.autoloadBytes)} sub="loaded on every request" tone={data.autoloadBytes > 800 * 1024 ? "orange" : undefined} />
        <Stat label="Transients" value={formatCount(data.transients.length)} />
        <Stat label="Expired" value={formatCount(expired.length)} tone={expired.length ? "orange" : undefined} sub={expired.length ? "cleanup ready" : undefined} />
      </div>
      <Card title="Autoload by name prefix" aside="click a prefix to list its options">
        <BreakdownChart items={owners} format={formatBytes} totalLabel="autoloaded" />
      </Card>
      <Card title="Largest autoloaded options" aside="click to open the row">
        <div className="rows">
          {data.largestAutoload.map((o) => (
            <button key={o.name} className="row-link" onClick={() => openRow(data.table, "option_name", o.name)}>
              <span className="grow-text mono">{o.name}</span>
              <span className="muted">{formatBytes(o.bytes)}</span>
            </button>
          ))}
        </div>
      </Card>
      {prefix && <OptionGroup key={prefix} prefix={prefix} />}
      <Card
        title="Transients"
        aside={
          <button className="btn" disabled={!expired.length} onClick={() => void queueChange({ type: "deleteExpiredTransients", database: session.database! })}>
            <Eraser size="0.923rem" /> Queue cleanup of {expired.length} expired
          </button>
        }
      >
        <div className="rows">
          {data.transients.map((t) => (
            <div key={`${t.siteWide}-${t.name}`} className="row-link" role="row">
              <span className="grow-text mono">{t.name}{t.siteWide && <span className="badge" style={{ marginLeft: 6 }}>site</span>}</span>
              <span className="muted">{formatBytes(t.bytes)}</span>
              <span className={t.expired ? "tone-text-orange row-when" : "muted row-when"}>
                {t.expiresAt == null ? "never expires" : <>{t.expired ? "expired " : "expires "}<Ago unix={t.expiresAt} /></>}
              </span>
            </div>
          ))}
        </div>
      </Card>
    </div>
  );
}
