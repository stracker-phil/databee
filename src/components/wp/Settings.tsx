import { useState } from "react";
import { Eraser } from "lucide-react";
import { api } from "../../api";
import { formatBytes, formatCount } from "../../lib/format";
import { selectActiveSession, selectWp, useStore } from "../../state/store";
import { ErrorPanel } from "../QueryTab";
import { Treemap } from "./charts";
import { FieldList } from "./FieldValue";
import { Ago, Card, Loading, Stat } from "./ui";
import { useWpData } from "./useWpData";

const PALETTE = ["var(--tile-blue)", "var(--tile-green)", "var(--tile-purple)", "var(--tile-orange)", "var(--tile-red)", "var(--tile-gray)"];

function OptionGroup({ prefix }: { prefix: string }) {
  const session = useStore(selectActiveSession);
  const queueChange = useStore((s) => s.queueChange);
  const wp = useStore(selectWp);
  const { data, error } = useWpData((s, d) => api.wpOptionsGroup(s, d, prefix), [prefix]);
  if (error) return <ErrorPanel message={error.message} code={error.code} />;
  if (!data || !session?.database || !wp) return <Loading />;
  return (
    <Card title={`Options: ${prefix}`} aside={`${data.length} options · click a pencil to edit, “autoload” to toggle`}>
      <div className="option-autoload">
        {data.map((f) => {
          const on = f.label?.endsWith("· autoload") ?? false;
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

export function SettingsScreen() {
  const session = useStore(selectActiveSession);
  const queueChange = useStore((s) => s.queueChange);
  const [prefix, setPrefix] = useState<string | null>(null);
  const { data, error } = useWpData(api.wpOptions, []);
  if (error) return <ErrorPanel message={error.message} code={error.code} />;
  if (!data || !session?.database) return <Loading />;
  const expired = data.transients.filter((t) => t.expired);
  const tiles = data.groups
    .filter((g) => g.autoloadBytes > 0)
    .slice(0, 24)
    .map((g, i) => ({
      key: g.prefix,
      label: g.prefix,
      value: g.autoloadBytes,
      sub: `${formatBytes(g.autoloadBytes)} autoloaded · ${g.count} options`,
      color: PALETTE[i % PALETTE.length],
    }));

  return (
    <div className="dash">
      <div className="stat-grid">
        <Stat label="Options" value={formatCount(data.totalCount)} />
        <Stat label="Autoloaded" value={formatBytes(data.autoloadBytes)} sub="loaded on every request" tone={data.autoloadBytes > 800 * 1024 ? "orange" : undefined} />
        <Stat label="Transients" value={formatCount(data.transients.length)} />
        <Stat label="Expired" value={formatCount(expired.length)} tone={expired.length ? "orange" : undefined} sub={expired.length ? "cleanup ready" : undefined} />
      </div>
      <Card title="Who bloats autoload? · click a tile">
        <Treemap items={tiles} height={220} onSelect={setPrefix} />
      </Card>
      {prefix && <OptionGroup key={prefix} prefix={prefix} />}
      <Card title="By prefix">
        <div className="rows">
          {data.groups.map((g) => (
            <button key={g.prefix} className={`row-link${prefix === g.prefix ? " selected" : ""}`} onClick={() => setPrefix(g.prefix)}>
              <span className="grow-text">{g.prefix}</span>
              <span className="muted">{formatCount(g.count)} options</span>
              <span className="muted">{formatBytes(g.bytes)}</span>
              <span className="money row-total">{g.autoloadCount ? `${formatBytes(g.autoloadBytes)} autoload` : "—"}</span>
            </button>
          ))}
        </div>
      </Card>
      <Card
        title="Transients"
        aside={
          <button className="btn" disabled={!expired.length} onClick={() => void queueChange({ type: "deleteExpiredTransients", database: session.database! })}>
            <Eraser size={12} /> Queue cleanup of {expired.length} expired
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
