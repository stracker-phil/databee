import { useState } from "react";
import { RefreshCw, Settings2 } from "lucide-react";
import type { WpInfo } from "../api";
import { selectActiveSession, selectWp, useStore } from "../state/store";

export function hposLabel(info: WpInfo): string | null {
  const wc = info.context.wc;
  if (!wc) return null;
  return wc.hpos === "off" ? "legacy orders" : wc.hpos === "on" ? "HPOS" : "HPOS + sync";
}

export function WordPressPanel() {
  const session = useStore(selectActiveSession);
  const wp = useStore(selectWp);
  const { openDocument, openOptions, loadWp } = useStore.getState();
  const [entity, setEntity] = useState("order");
  const [id, setId] = useState("");

  if (!session?.database) return <div className="sidebar-empty">Select a database first.</div>;
  if (wp === undefined) return <div className="sidebar-empty">Detecting WordPress…</div>;
  if (wp === null) return <div className="sidebar-empty">{session.database} is not a WordPress database.</div>;

  const ctx = wp.context;
  const selected = wp.entities.some(([n]) => n === entity) ? entity : wp.entities[0]?.[0];
  return (
    <div className="wp-panel">
      <dl>
        <dt>Prefix</dt><dd className="mono">{ctx.prefix}</dd>
        {ctx.version && (<><dt>DB version</dt><dd>{ctx.version}</dd></>)}
        <dt>WooCommerce</dt><dd>{ctx.wc ? (ctx.wc.version ?? "yes") : "no"}</dd>
        {ctx.wc && (<><dt>Orders</dt><dd>{hposLabel(wp)}</dd></>)}
        {ctx.wc && (<><dt>Action Scheduler</dt><dd>{ctx.wc.hasActionScheduler ? "yes" : "no"}</dd></>)}
        {ctx.sites.length > 0 && (<><dt>Multisite</dt><dd>{ctx.sites.length + 1} sites</dd></>)}
      </dl>

      <form
        className="field"
        onSubmit={(e) => {
          e.preventDefault();
          const label = wp.entities.find(([n]) => n === selected)?.[1];
          if (selected && /^\d+$/.test(id.trim())) openDocument(selected, id.trim(), label);
        }}
      >
        <span>Open by ID</span>
        <div style={{ display: "flex", gap: 6 }}>
          <select className="input" style={{ width: 120 }} aria-label="Entity" value={selected} onChange={(e) => setEntity(e.target.value)}>
            {wp.entities.map(([name, label]) => (
              <option key={name} value={name}>{label}</option>
            ))}
          </select>
          <input className="input" aria-label="ID" inputMode="numeric" placeholder="ID" value={id} onChange={(e) => setId(e.target.value)} />
        </div>
        <button className="btn btn-primary" type="submit" disabled={!/^\d+$/.test(id.trim())}>Open document</button>
      </form>

      <button className="btn" onClick={openOptions}>
        <Settings2 size={13} /> Options inspector
      </button>

      {ctx.sites.length > 0 && (
        <div className="field">
          <span>Sub-sites</span>
          {ctx.sites.map((s) => (
            <div key={s.blogId} className="hint mono">
              {s.prefix} {s.domain ? `${s.domain}${s.path ?? ""}` : ""}
            </div>
          ))}
        </div>
      )}

      <button className="btn btn-ghost" onClick={() => void loadWp(session.id, session.database!, true)}>
        <RefreshCw size={12} /> Detect again
      </button>
    </div>
  );
}
