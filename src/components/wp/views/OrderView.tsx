import { useEffect, useState } from "react";
import { AlertTriangle, ExternalLink } from "lucide-react";
import { api, type OrderView as OrderViewData, type TimelineEvent } from "../../../api";
import { absoluteFromUnix } from "../../../lib/wpFormat";
import { selectActiveSession, useStore } from "../../../state/store";
import { LinkChip } from "../LinkChip";
import { Ago, Card, StatusPill, Thumb } from "../ui";
import { AddressCard, AllFields, RelationStrip, Timeline } from "./common";

const ORDER_STATUSES = ["pending", "processing", "on-hold", "completed", "cancelled", "refunded", "failed"];
const SUBSCRIPTION_STATUSES = ["pending", "active", "on-hold", "pending-cancel", "cancelled", "expired"];

function StatusSelect({ order }: { order: OrderViewData }) {
  const session = useStore(selectActiveSession);
  const queueChange = useStore((s) => s.queueChange);
  const statuses = order.kind === "shop_subscription" ? SUBSCRIPTION_STATUSES : ORDER_STATUSES;
  const current = order.status.replace(/^wc-/, "");
  return (
    <span className="status-select">
      <StatusPill status={order.status} />
      <select
        aria-label="Change status"
        value={current}
        onChange={(e) =>
          session?.database &&
          void queueChange({ type: "setOrderStatus", database: session.database, id: order.doc.id, status: e.target.value })
        }
      >
        {!statuses.includes(current) && <option value={current}>{current}</option>}
        {statuses.map((s) => (
          <option key={s} value={s}>{s}</option>
        ))}
      </select>
    </span>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return children == null || children === "" ? null : (
    <div className="kv-row">
      <div className="k">{label}</div>
      <div className="v">{children}</div>
    </div>
  );
}

export function OrderView({ o, changed }: { o: OrderViewData; changed: Set<string> }) {
  const session = useStore(selectActiveSession);
  const [logs, setLogs] = useState<TimelineEvent[] | null>(null);
  useEffect(() => {
    if (!session?.database) return;
    let alive = true;
    api.wpOrderLogs(session.id, session.database, o.doc.id).then((l) => alive && setLogs(l), () => alive && setLogs([]));
    return () => {
      alive = false;
    };
  }, [session?.id, session?.database, o.doc.id]);
  const timeline = [...o.timeline, ...(logs ?? [])].sort((a, b) => a.at - b.at);
  const subscription = o.kind === "shop_subscription";
  const p = o.payment;

  return (
    <div className="entity order-layout">
      <div className="order-main">
      <div className="entity-head">
        <span className="badge">{subscription ? "Subscription" : o.kind === "shop_order_refund" ? "Refund" : "Order"}</span>
        <h2>#{o.doc.id}</h2>
        <StatusSelect order={o} />
        <span className="money big">{o.total}</span>
        {p?.gatewayTitle && <span className="muted">{p.gatewayTitle}</span>}
        {o.customer ? <LinkChip link={o.customer} /> : <span className="badge">Guest</span>}
        <div className="spacer" />
        {o.created && <span className="muted" title={absoluteFromUnix(o.created)}>created <Ago unix={o.created} /></span>}
      </div>
      <RelationStrip items={o.related} onTarget={() => document.getElementById("order-timeline")?.scrollIntoView({ behavior: "smooth" })} />

      {o.addressNotes.length > 0 && (
        <div className="insight warning">
          <AlertTriangle size="1rem" /> {o.addressNotes.join(" · ")}
        </div>
      )}
      <div className="address-grid">
        <AddressCard title="Billing" address={o.billing} warn={o.addressNotes.length > 0} />
        <AddressCard title="Shipping" address={o.shipping} warn={o.addressNotes.length > 0} />
      </div>

      <Card title="Items">
        <div className="items">
          {o.items
            .filter((i) => i.kind !== "tax")
            .map((i) => (
              <div key={i.id} className="item-row">
                <Thumb src={i.product?.image} size="3.077rem" />
                <div className="grow-text">
                  <div>
                    {i.product ? <button className="link-btn plain" onClick={() => useStore.getState().openEntity(i.product!.entity, i.product!.id)}>{i.name}</button> : i.name}
                    {i.kind !== "line_item" && <span className="badge" style={{ marginLeft: 6 }}>{i.kind.replace("_", " ")}</span>}
                  </div>
                  <div className="muted small">
                    {[i.sku && <span key="sku" className="mono">{i.sku}</span>, ...i.meta.map(([k, v]) => <span key={k}>{k}: {v}</span>)]
                      .filter(Boolean)
                      .map((el, n) => (
                        <span key={n}>{n > 0 && " · "}{el}</span>
                      ))}
                  </div>
                </div>
                {i.quantity && <span className="muted">× {i.quantity}</span>}
                <span className="money">{i.total}</span>
              </div>
            ))}
        </div>
        <div className="totals">
          {o.totals.map(([label, value]) => (
            <div key={label} className={label === "Total" ? "total-row strong" : "total-row"}>
              <span>{label}</span>
              <span className="money">{value}</span>
            </div>
          ))}
        </div>
      </Card>

      <div className="dash-row">
        {p && (
          <Card title="Payment" className="grow">
            <div className="kv">
              <Row label="Gateway">{p.gatewayTitle ?? p.gateway}</Row>
              <Row label="Transaction ID"><span className="mono">{p.transactionId}</span></Row>
              <Row label="PayPal order ID"><span className="mono">{p.paypalOrderId}</span></Row>
              <Row label="Intent">{p.intent}</Row>
              <Row label="Mode">{p.mode && <span className={`pill tone-${p.mode === "live" ? "green" : "orange"}`}>{p.mode}</span>}</Row>
              <Row label="Source">{p.source}</Row>
              <Row label="Payer">{p.payerEmail && <span className="pii">{p.payerEmail}</span>}</Row>
              <Row label="Paid">{o.paid && <Ago unix={o.paid} />}</Row>
            </div>
            {p.fees && (
              <div className="waterfall" aria-label="Fees">
                <div className="wf-bar">
                  <span className="wf-net" style={{ width: `${(1 - p.fees.feeRatio) * 100}%` }} />
                  <span className="wf-fee" style={{ width: `${p.fees.feeRatio * 100}%` }} />
                </div>
                <div className="wf-labels">
                  <span>gross <strong>{p.fees.gross}</strong></span>
                  <span>→ fee <strong className="tone-text-red">{p.fees.fee}</strong></span>
                  <span>→ net <strong className="tone-text-green">{p.fees.net}</strong></span>
                </div>
              </div>
            )}
          </Card>
        )}
        {o.attribution && (
          <Card title="Attribution" className="grow">
            <div className="kv">
              <Row label="Source">{[o.attribution.sourceType, o.attribution.source].filter(Boolean).join(" · ")}</Row>
              <Row label="Referrer">{o.attribution.referrer}</Row>
              <Row label="Entry page">
                {o.attribution.entry && (
                  <a className="url" href={o.attribution.entry} onClick={(e) => (e.preventDefault(), void api.openUrl(o.attribution!.entry!))}>
                    {o.attribution.entry.replace(/^https?:\/\//, "")} <ExternalLink size="0.769rem" />
                  </a>
                )}
              </Row>
              <Row label="Session">{o.attribution.pages && `${o.attribution.pages} pages · ${o.attribution.sessions ?? "?"} sessions`}</Row>
              <Row label="Device">{[o.attribution.device, o.attribution.userAgent].filter(Boolean).join(" · ")}</Row>
              <Row label="Started">{o.attribution.sessionStart && <Ago unix={o.attribution.sessionStart} />}</Row>
              {o.attribution.utm.map(([k, v]) => (
                <Row key={k} label={`UTM ${k}`}>{v}</Row>
              ))}
            </div>
          </Card>
        )}
      </div>

      <AllFields doc={o.doc} changed={changed} />
      </div>
      <aside className="order-rail">
        <Card title="Timeline" aside={logs == null ? "loading logs…" : `${timeline.length} events`}>
          <div id="order-timeline">
            <Timeline events={timeline} />
          </div>
        </Card>
      </aside>
    </div>
  );
}
