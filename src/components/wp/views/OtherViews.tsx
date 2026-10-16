import { CreditCard, Play, XCircle } from "lucide-react";
import type { ActionView as ActionData, CustomerView as CustomerData, EntityDoc, ProductView as ProductData, SessionView as SessionData } from "../../../api";
import { selectActiveSession, useStore } from "../../../state/store";
import { ValueTree } from "../../ValueTree";
import { LinkChip } from "../LinkChip";
import { Ago, Avatar, Card, Stat, StatusPill, Thumb } from "../ui";
import { AddressCard, AllFields, RelationStrip } from "./common";

function OrdersMini({ orders, title }: { orders: CustomerData["orders"]; title: string }) {
  const openEntity = useStore((s) => s.openEntity);
  if (orders.length === 0) return null;
  return (
    <Card title={title}>
      <div className="rows">
        {orders.map((o) => (
          <button key={o.id} className="row-link" onClick={() => openEntity("order", o.id)}>
            <span className="row-id">#{o.id}</span>
            <StatusPill status={o.status} />
            <span className="grow-text muted mono" title={o.gatewayTitle ?? undefined}>{o.gateway}</span>
            <span className="muted">{o.itemCount} items</span>
            <span className="money row-total">{o.total}</span>
            <span className="muted row-when"><Ago unix={o.created} /></span>
          </button>
        ))}
      </div>
    </Card>
  );
}

export function CustomerView({ c, changed }: { c: CustomerData; changed: Set<string> }) {
  const navigate = useStore((s) => s.navigate);
  return (
    <div className="entity">
      <div className="entity-head">
        <Avatar name={c.name} src={c.avatar} size="3.385rem" />
        <div>
          <h2>{c.name}</h2>
          <div className="muted">{[c.login, c.email].filter(Boolean).join(" · ")}</div>
        </div>
        {c.roles.map((r) => (
          <span key={r} className="badge">{r}</span>
        ))}
        <div className="spacer" />
        <span className="muted">registered <Ago unix={c.registered} /></span>
      </div>
      <RelationStrip items={c.related} onTarget={(t) => t.startsWith("list:orders") && navigate({ screen: "list", list: "orders", params: { search: c.email ?? c.doc.id } })} />
      <div className="stat-grid">
        <Stat label="Orders" value={c.orderCount} />
        <Stat label="Total spent" value={c.totalSpent ?? "—"} />
        <Stat label="Average order" value={c.averageOrder ?? "—"} />
        <Stat label="Last active" value={<Ago unix={c.lastActive} />} />
      </div>
      <div className="address-grid">
        <AddressCard title="Billing" address={c.billing} />
        <AddressCard title="Shipping" address={c.shipping} />
      </div>
      <OrdersMini orders={c.orders} title="Latest orders" />
      <OrdersMini orders={c.subscriptions} title="Subscriptions" />
      {c.tokens.length > 0 && (
        <Card title="Saved payment methods">
          <div className="rows">
            {c.tokens.map((t) => (
              <div key={t.id} className="row-link">
                <CreditCard size="1.154rem" />
                <span className="grow-text">{t.summary}</span>
                <span className="muted mono">{t.gateway}</span>
                {t.isDefault && <span className="pill tone-green">default</span>}
              </div>
            ))}
          </div>
        </Card>
      )}
      <AllFields doc={c.doc} changed={changed} />
    </div>
  );
}

export function ProductView({ p, changed }: { p: ProductData; changed: Set<string> }) {
  const attrNames = [...new Set(p.variations.flatMap((v) => v.attributes.map(([k]) => k)))];
  return (
    <div className="entity">
      <div className="product-head">
        <Thumb src={p.image} size="9.231rem" />
        <div className="grow-text">
          <div className="entity-head" style={{ padding: 0 }}>
            <span className="badge">{p.kind ?? "product"}</span>
            <h2>{p.name}</h2>
            <StatusPill status={p.status} />
          </div>
          <div className="product-facts">
            <span className="money big">
              {p.salePrice ? (
                <>
                  <s className="muted">{p.regularPrice}</s> {p.salePrice}
                </>
              ) : (
                p.price
              )}
            </span>
            {p.sku && <span className="mono muted">SKU {p.sku}</span>}
            {p.stockStatus && <StatusPill status={p.stockStatus} label={`${p.stockStatus}${p.stock ? ` · ${p.stock}` : ""}`} />}
            {p.totalSales && <span className="muted">{p.totalSales} sold</span>}
          </div>
          <div className="product-terms">
            {p.terms.map(([tax, names]) => (
              <span key={tax}>
                <span className="muted">{tax}: </span>
                {names.map((n) => (
                  <span key={n} className="badge" style={{ marginRight: 4 }}>{n}</span>
                ))}
              </span>
            ))}
            {p.attributes.map(([name, values]) => (
              <span key={name}>
                <span className="muted">{name}: </span>
                {values.join(", ")}
              </span>
            ))}
          </div>
        </div>
      </div>
      <RelationStrip items={p.related} />
      {p.variations.length > 0 && (
        <Card title="Variations">
          <table className="matrix">
            <thead>
              <tr>
                <th />
                {attrNames.map((a) => (
                  <th key={a}>{a}</th>
                ))}
                <th>SKU</th>
                <th>Price</th>
                <th>Stock</th>
              </tr>
            </thead>
            <tbody>
              {p.variations.map((v) => (
                <tr key={v.id}>
                  <td><Thumb src={v.image ?? p.image} size="2.154rem" /></td>
                  {attrNames.map((a) => (
                    <td key={a}>{v.attributes.find(([k]) => k === a)?.[1] ?? "Any"}</td>
                  ))}
                  <td className="mono">{v.sku}</td>
                  <td className="money">{v.price}</td>
                  <td>{v.stockStatus && <StatusPill status={v.stockStatus} label={`${v.stockStatus}${v.stock ? ` · ${v.stock}` : ""}`} />}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}
      <AllFields doc={p.doc} changed={changed} />
    </div>
  );
}

export function ActionView({ a, changed }: { a: ActionData; changed: Set<string> }) {
  const session = useStore(selectActiveSession);
  const queueChange = useStore((s) => s.queueChange);
  const db = session?.database;
  return (
    <div className="entity">
      <div className="entity-head">
        <span className="badge">Scheduled action</span>
        <h2 className="mono">{a.hook}</h2>
        <StatusPill status={a.status} />
        {a.group && <span className="badge">{a.group}</span>}
        <div className="spacer" />
        <span className="muted">scheduled <Ago unix={a.scheduled} /></span>
        {db && a.status !== "pending" && (
          <button className="btn" onClick={() => void queueChange({ type: "retryAction", database: db, id: a.doc.id })}>
            <Play size="0.923rem" /> Retry
          </button>
        )}
        {db && a.status === "pending" && (
          <button className="btn" onClick={() => void queueChange({ type: "cancelAction", database: db, id: a.doc.id })}>
            <XCircle size="0.923rem" /> Cancel
          </button>
        )}
      </div>
      {a.args && (
        <Card title="Arguments">
          <ValueTree value={a.args} />
        </Card>
      )}
      <Card title="Log">
        <ol className="timeline">
          {a.logs.map((l, i) => (
            <li key={i} className={`tl-item tl-${/fail|error|exception/i.test(l.message) ? "error" : "info"}`}>
              <span className="tl-icon">•</span>
              <div className="tl-body">
                <div className="tl-title">{l.message}</div>
                <div className="tl-meta"><Ago unix={l.at} /></div>
              </div>
            </li>
          ))}
        </ol>
      </Card>
      <AllFields doc={a.doc} changed={changed} />
    </div>
  );
}

export function SessionView({ s, changed }: { s: SessionData; changed: Set<string> }) {
  const expired = s.expires != null && s.expires < Date.now() / 1000;
  return (
    <div className="entity">
      <div className="entity-head">
        <span className="badge">Session</span>
        <h2 className="mono">{s.key}</h2>
        {s.customer ? <LinkChip link={s.customer} /> : <span className="badge">Guest</span>}
        <div className="spacer" />
        <span className={expired ? "tone-text-red" : "muted"}>
          {expired ? "expired " : "expires "}
          <Ago unix={s.expires} />
        </span>
      </div>
      <div className="stat-grid">
        <Stat label="Items in cart" value={s.items} />
        <Stat label="Cart total" value={s.total ?? "—"} />
        <Stat label="Customer email" value={<span>{s.email ?? "—"}</span>} />
      </div>
      {s.value && (
        <Card title="Session data">
          <ValueTree value={s.value} />
        </Card>
      )}
      <AllFields doc={s.doc} changed={changed} />
    </div>
  );
}

export function GenericView({ doc, changed }: { doc: EntityDoc; changed: Set<string> }) {
  return (
    <div className="entity">
      <div className="entity-head">
        {doc.image && <Thumb src={doc.image} size="3.385rem" />}
        <span className="badge">{doc.label}</span>
        <h2>#{doc.id} {doc.title && <span style={{ fontWeight: 400 }}>{doc.title}</span>}</h2>
        {doc.subtitle && <span className="badge">{doc.subtitle}</span>}
      </div>
      <AllFields doc={doc} changed={changed} />
    </div>
  );
}
