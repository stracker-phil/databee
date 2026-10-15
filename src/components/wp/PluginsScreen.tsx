import { useState } from "react";
import { Check, Eraser, HelpCircle, Puzzle, RotateCcw } from "lucide-react";
import { api, type LensKey, type PluginReport } from "../../api";
import { formatBytes, formatCount } from "../../lib/format";
import { selectActiveSession, useStore } from "../../state/store";
import { ErrorPanel } from "../QueryTab";
import { BreakdownChart } from "./BreakdownChart";
import { Ago, Card, Loading, Stat } from "./ui";
import { useWpData } from "./useWpData";

/** Key ids the cleanup change may delete: options, transients and meta rows. */
export function deletableKeys(p: PluginReport): string[] {
  return p.keys.filter((k) => k.kind === "option" || k.kind === "transient" || k.kind === "meta").map(keyId);
}

export function keyId(k: LensKey): string {
  switch (k.kind) {
    case "option":
      return `option:${k.name}`;
    case "transient":
      return `transient:${k.name}`;
    case "meta":
      return `meta:${k.table}:${k.name}`;
    case "hook":
      return `hook:${k.name}`;
    case "log_source":
      return `log:${k.name}`;
    default:
      return `table:${k.name}`;
  }
}

const KIND_LABEL: Record<LensKey["kind"], string> = {
  option: "Options",
  transient: "Transients",
  meta: "Meta keys",
  hook: "Scheduled hooks",
  log_source: "Log sources",
  table: "Custom tables",
};

function ConfidenceMark({ k, onAssign, onReset }: { k: LensKey; onAssign: () => void; onReset: () => void }) {
  if (k.source === "manual") {
    return (
      <span className="confidence">
        <span className="badge">manual</span>
        <button className="icon-btn tiny" aria-label={`Reset assignment of ${k.name}`} title="Remove manual assignment" onClick={onReset}>
          <RotateCcw size="0.769rem" />
        </button>
      </span>
    );
  }
  if (k.confidence === "exact") return <span className="confidence muted" title="Exact: from the registry"><Check size="0.846rem" /></span>;
  return (
    <span className="confidence">
      <span className="badge tone-warning" title="Guessed from the key prefix">{k.confidence ?? "unknown"}</span>
      <button className="link-btn plain small" onClick={onAssign}>
        {k.plugin ? "confirm / reassign…" : "assign…"}
      </button>
    </span>
  );
}

function KeyRows({ keys, onAssign, onReset }: { keys: LensKey[]; onAssign: (k: LensKey, confirmSlug?: string) => void; onReset: (k: LensKey) => void }) {
  return (
    <div className="rows">
      {keys.map((k) => (
        <div key={keyId(k)} className="row-link" role="row">
          <span className="mono grow-text">
            {k.name}
            {k.kind === "meta" && <span className="muted small"> · {k.table}</span>}
          </span>
          {k.autoload && <span className="badge">autoload</span>}
          {k.kind !== "option" && k.kind !== "transient" && <span className="muted row-items">{formatCount(k.count)} {k.kind === "table" ? "rows" : k.kind === "hook" ? "actions" : k.kind === "log_source" ? "entries" : "rows"}</span>}
          <span className="muted row-total">{k.bytes ? formatBytes(k.bytes) : ""}</span>
          <span className="row-when">{k.lastWritten ? <Ago unix={k.lastWritten} /> : ""}</span>
          <ConfidenceMark k={k} onAssign={() => onAssign(k)} onReset={() => onReset(k)} />
        </div>
      ))}
    </div>
  );
}

function PluginDetail({ p, onAssign, onReset, onBack }: { p: PluginReport; onAssign: (k: LensKey) => void; onReset: (k: LensKey) => void; onBack: () => void }) {
  const session = useStore(selectActiveSession);
  const queueChange = useStore((s) => s.queueChange);
  const groups = (["option", "transient", "meta", "hook", "log_source", "table"] as const).map((kind) => ({ kind, keys: p.keys.filter((k) => k.kind === kind) })).filter((g) => g.keys.length > 0);
  const deletable = deletableKeys(p);
  return (
    <div className="dash">
      <div className="dash-head">
        <button className="link-btn plain" onClick={onBack}>← Plugins</button>
        <h2>{p.name}</h2>
        {p.active ? <span className="pill tone-green">active</span> : <span className="pill tone-orange">not active · leftover data</span>}
        <span className="muted mono small">{p.slug}</span>
        <div className="spacer" />
        {!p.active && deletable.length > 0 && session?.database && (
          <button className="btn" onClick={() => void queueChange({ type: "deletePluginData", database: session.database!, slug: p.slug, keys: deletable })}>
            <Eraser size="0.923rem" /> Queue cleanup of {deletable.length} keys
          </button>
        )}
      </div>
      <div className="stat-grid">
        <Stat label="Storage" value={formatBytes(p.totalBytes)} sub="options, meta, logs, tables" />
        <Stat label="Autoload" value={formatBytes(p.autoloadBytes)} sub={`${formatCount(p.optionCount)} options`} tone={p.autoloadBytes > 100 * 1024 ? "orange" : undefined} />
        <Stat label="Meta rows" value={formatCount(p.meta.reduce((s, m) => s + m.rows, 0))} sub={p.meta.map((m) => `${m.table} ${formatCount(m.keys)}`).join(" · ") || "none"} />
        <Stat label="Scheduled actions" value={formatCount(p.hookCount)} sub={p.logEntries ? `${formatCount(p.logEntries)} log entries` : undefined} />
        {p.orders > 0 && <Stat label="Orders paid via" value={formatCount(p.orders)} sub="gateway attribution" />}
      </div>
      {groups.map((g) => (
        <Card key={g.kind} title={KIND_LABEL[g.kind]} aside={g.kind === "table" ? "never dropped automatically" : `${g.keys.length}`}>
          <KeyRows keys={g.keys} onAssign={onAssign} onReset={onReset} />
        </Card>
      ))}
    </div>
  );
}

export function PluginsScreen({ params }: { params?: Record<string, string> }) {
  const { navigate, replaceRoute, openDialog, bumpData, toast } = useStore.getState();
  const session = useStore(selectActiveSession);
  const queueChange = useStore((s) => s.queueChange);
  const { data, error } = useWpData(api.wpLens, []);
  const [busy, setBusy] = useState(false);
  if (error) return <ErrorPanel message={error.message} code={error.code} />;
  if (!data) return <Loading />;

  const assign = (k: LensKey, slug: string | null) => {
    setBusy(true);
    api.wpPluginAssign(keyId(k), slug).then(
      () => {
        setBusy(false);
        bumpData();
      },
      (e) => {
        setBusy(false);
        toast("error", String((e as { message?: string }).message ?? e));
      },
    );
  };
  const askAssign = (k: LensKey) =>
    openDialog({
      type: "prompt",
      title: `Plugin slug for ${k.name}`,
      value: k.plugin ?? "",
      onSubmit: (slug) => slug.trim() && assign(k, slug.trim()),
    });
  const reset = (k: LensKey) => assign(k, null);

  const selected = params?.slug ? data.plugins.find((p) => p.slug === params.slug) : null;
  if (selected) {
    return <PluginDetail p={selected} onAssign={askAssign} onReset={reset} onBack={() => navigate({ screen: "list", list: "plugins" })} />;
  }

  const leftovers = data.plugins.filter((p) => !p.active && deletableKeys(p).length > 0);
  const items = data.plugins
    .filter((p) => p.totalBytes > 0)
    .map((p) => ({ id: p.slug, label: p.name, value: p.totalBytes, sublabel: p.active ? undefined : "not active", onOpen: () => replaceRoute({ screen: "list", list: "plugins", params: { slug: p.slug, name: p.name } }) }));
  const open = (p: PluginReport) => navigate({ screen: "list", list: "plugins", params: { slug: p.slug, name: p.name } });

  return (
    <div className={`dash${busy ? " stale" : ""}`}>
      <div className="stat-grid">
        <Stat label="Active plugins" value={formatCount(data.activePlugins.length)} />
        <Stat label="Plugins with data" value={formatCount(data.plugins.length)} />
        <Stat label="Leftover data" value={formatBytes(leftovers.reduce((s, p) => s + p.totalBytes, 0))} sub={leftovers.length ? `${leftovers.length} inactive ${leftovers.length === 1 ? "plugin" : "plugins"}` : "none"} tone={leftovers.length ? "orange" : undefined} />
        <Stat label="Unattributed keys" value={formatCount(data.unattributed.length)} sub={data.unattributed.length ? "assign them below" : "everything has an owner"} />
      </div>

      {leftovers.length > 0 && (
        <Card title="Leftover data" aside="plugins that are not active but still store data">
          <div className="rows">
            {leftovers.map((p) => {
              const keys = deletableKeys(p);
              return (
                <div key={p.slug} className="row-link" role="row">
                  <button className="link-btn plain grow-text" onClick={() => open(p)}>{p.name}</button>
                  <span className="muted row-items">{formatCount(keys.length)} keys</span>
                  <span className="muted row-total">{formatBytes(p.totalBytes)}</span>
                  {session?.database && (
                    <button className="btn tiny" onClick={() => void queueChange({ type: "deletePluginData", database: session.database!, slug: p.slug, keys })}>
                      <Eraser size="0.846rem" /> Queue cleanup
                    </button>
                  )}
                </div>
              );
            })}
          </div>
          <div className="hint">Cleanup deletes options, transients and meta rows only; tables and log entries stay for manual review. Everything goes through the pending tray first.</div>
        </Card>
      )}

      <Card title="Storage by plugin" aside="click a plugin for its keys">
        <BreakdownChart items={items} format={formatBytes} totalLabel="attributed" />
      </Card>

      <Card title="All plugins">
        <div className="rows" role="grid" aria-label="plugins list">
          <div className="list-head" role="row">
            <span role="columnheader" className="grow-text">Plugin</span>
            <span role="columnheader" className="row-gateway">Options</span>
            <span role="columnheader" className="row-gateway">Meta</span>
            <span role="columnheader" className="row-items">Hooks</span>
            <span role="columnheader" className="row-total">Autoload</span>
            <span role="columnheader" className="row-total">Storage</span>
            <span role="columnheader" className="row-when">Last written</span>
          </div>
          {data.plugins.map((p) => (
            <button key={p.slug} className="row-link" role="row" onClick={() => open(p)}>
              <span className="grow-text">
                <Puzzle size="0.923rem" className="muted" style={{ verticalAlign: -2, marginRight: "0.385rem" }} />
                {p.name}
                {!p.active && <span className="badge tone-warning" style={{ marginLeft: "0.462rem" }}>not active</span>}
              </span>
              <span className="muted row-gateway">{formatCount(p.optionCount)} opt · {formatCount(p.transientCount)} tr</span>
              <span className="muted row-gateway">{formatCount(p.meta.reduce((s, m) => s + m.keys, 0))} keys · {formatCount(p.meta.reduce((s, m) => s + m.rows, 0))} rows</span>
              <span className="muted row-items">{formatCount(p.hookCount)}</span>
              <span className="row-total">{p.autoloadBytes ? formatBytes(p.autoloadBytes) : "—"}</span>
              <span className="money row-total">{formatBytes(p.totalBytes)}</span>
              <span className="muted row-when">{p.lastWritten ? <Ago unix={p.lastWritten} /> : "—"}</span>
            </button>
          ))}
        </div>
      </Card>

      {data.unattributed.length > 0 && (
        <Card title="Unattributed keys" aside={<><HelpCircle size="0.846rem" /> no plugin claims these; assign them to teach the registry</>}>
          <KeyRows keys={data.unattributed.slice(0, 100)} onAssign={askAssign} onReset={reset} />
          {data.unattributed.length > 100 && <div className="hint">Showing the 100 largest of {formatCount(data.unattributed.length)}.</div>}
        </Card>
      )}
    </div>
  );
}
