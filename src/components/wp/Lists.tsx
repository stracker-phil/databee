import { useEffect, useState, type ReactNode } from "react";
import { AlertTriangle, ChevronLeft, ChevronRight, Search } from "lucide-react";
import {
  api,
  type ActionSummary,
  type CustomerSummary,
  type Facet,
  type ListQuery,
  type OrderSummary,
  type Page,
  type PostSummary,
  type ProductSummary,
  type SessionSummary,
} from "../../api";
import { formatCount } from "../../lib/format";
import { relativeFromUnix, shorten } from "../../lib/wpFormat";
import { useStore, type WpList } from "../../state/store";
import { ErrorPanel } from "../QueryTab";
import { BreakdownChart } from "./BreakdownChart";
import { StackedBars } from "./charts";
import { Ago, Avatar, Card, StatusPill, Thumb } from "./ui";
import { useWpData } from "./useWpData";

const PAGE = 50;

type Loader = (sessionId: string, database: string, q: ListQuery) => Promise<Page<unknown>>;

const LOADERS: Record<Exclude<WpList, "settings">, Loader> = {
  orders: (s, d, q) => api.wpOrders(s, d, "shop_order", q),
  subscriptions: (s, d, q) => api.wpOrders(s, d, "shop_subscription", q),
  customers: api.wpCustomers,
  products: api.wpProducts,
  actions: api.wpActions,
  sessions: api.wpSessions,
  content: api.wpContent,
};

const KIND_LABEL: Partial<Record<WpList, string>> = {
  orders: "Gateway",
  subscriptions: "Gateway",
  customers: "Role",
  products: "Type",
  actions: "Hook",
  content: "Type",
};

function useDebounced<T>(value: T, ms = 250): T {
  const [v, setV] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setV(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return v;
}

function Chips({ facets, value, onChange, allLabel = "All" }: { facets: Facet[]; value: string | null; onChange: (v: string | null) => void; allLabel?: string }) {
  if (facets.length === 0) return null;
  const total = facets.reduce((s, f) => s + f.count, 0);
  return (
    <div className="chips" role="group">
      <button className={`chip-filter${value == null ? " on" : ""}`} onClick={() => onChange(null)}>
        {allLabel} <span>{formatCount(total)}</span>
      </button>
      {facets.map((f) => (
        <button key={f.value} className={`chip-filter${value === f.value ? " on" : ""}`} onClick={() => onChange(value === f.value ? null : f.value)}>
          {f.label} <span>{formatCount(f.count)}</span>
        </button>
      ))}
    </div>
  );
}

function OrderRow({ o }: { o: OrderSummary }) {
  const openEntity = useStore((s) => s.openEntity);
  return (
    <button className="row-link" onClick={() => openEntity("order", o.id)}>
      <span className="row-id">#{o.id}</span>
      <span className="grow-text">
        <span className="pii">{o.name || o.email || <span className="badge">Guest</span>}</span>
        {o.name && o.email && <span className="muted pii"> · {o.email}</span>}
      </span>
      <StatusPill status={o.status} />
      <span className="row-gateway">{o.gatewayTitle ?? o.gateway ?? ""}</span>
      <span className="muted row-items">{o.itemCount} {o.itemCount === 1 ? "item" : "items"}</span>
      <span className="money row-total">{o.total}</span>
      <span className="muted row-when"><Ago unix={o.created} /></span>
    </button>
  );
}

function CustomerRow({ c }: { c: CustomerSummary }) {
  const openEntity = useStore((s) => s.openEntity);
  return (
    <button className="row-link" onClick={() => openEntity("user", c.id)}>
      <Avatar name={c.name || c.login} src={c.avatar} />
      <span className="grow-text">
        <span className="pii">{c.name || c.login}</span>
        <span className="muted pii"> · {c.email}</span>
      </span>
      <span className="row-roles">{c.roles.map((r) => <span key={r} className="badge">{r}</span>)}</span>
      <span className="muted row-items">{c.orderCount} {c.orderCount === 1 ? "order" : "orders"}</span>
      <span className="money row-total">{c.totalSpent ?? ""}</span>
      <span className="muted row-when"><Ago unix={c.lastOrder ?? c.registered} /></span>
    </button>
  );
}

function ProductRow({ p }: { p: ProductSummary }) {
  const openEntity = useStore((s) => s.openEntity);
  return (
    <button className="row-link" onClick={() => openEntity("post", p.id)}>
      <Thumb src={p.image} />
      <span className="grow-text">
        {p.name}
        {p.sku && <span className="muted mono"> · {p.sku}</span>}
      </span>
      {p.kind && <span className="badge">{p.kind}</span>}
      {p.variations > 0 && <span className="muted">{p.variations} variations</span>}
      {p.stockStatus && <StatusPill status={p.stockStatus} label={p.stockStatus === "instock" ? `In stock${p.stock ? ` · ${p.stock}` : ""}` : undefined} />}
      {p.status !== "publish" && <StatusPill status={p.status} />}
      <span className="muted row-items">{p.totalSales} sold</span>
      <span className="money row-total">{p.price}</span>
    </button>
  );
}

function ActionRow({ a }: { a: ActionSummary }) {
  const openEntity = useStore((s) => s.openEntity);
  return (
    <button className="row-link" onClick={() => openEntity("action", a.id)} title={a.message ?? undefined}>
      <StatusPill status={a.status} />
      <span className="grow-text">
        <span className="mono">{a.hook}</span>
        {a.args && <span className="muted mono"> {shorten(a.args, 40, 8)}</span>}
        {a.status === "failed" && a.message && (
          <span className="row-sub tone-text-red"><AlertTriangle size="0.846rem" /> {shorten(a.message, 90, 10)}</span>
        )}
      </span>
      {a.group && <span className="badge">{a.group}</span>}
      <span className="muted row-items">{a.attempts}×</span>
      <span className="muted row-when"><Ago unix={a.scheduled} /></span>
    </button>
  );
}

function SessionRow({ s }: { s: SessionSummary }) {
  const openEntity = useStore((st) => st.openEntity);
  const expired = s.expires != null && s.expires < Date.now() / 1000;
  return (
    <div className="row-link" role="row">
      <span className="mono grow-text">{shorten(s.key, 20, 4)}</span>
      {s.customerId ? (
        <button className="chip" onClick={() => openEntity("user", s.customerId!)}>
          <span className="chip-label pii">{s.email ?? `User #${s.customerId}`}</span>
        </button>
      ) : (
        <span className="badge">Guest</span>
      )}
      <span className="muted row-items">{s.items} in cart</span>
      <span className="money row-total">{s.total}</span>
      <span className={`row-when ${expired ? "tone-text-red" : "muted"}`}>
        {expired ? "expired " : "expires "}
        <Ago unix={s.expires} />
      </span>
    </div>
  );
}

function PostRow({ p }: { p: PostSummary }) {
  const openEntity = useStore((s) => s.openEntity);
  return (
    <button className="row-link" onClick={() => openEntity("post", p.id)}>
      <span className="row-id">#{p.id}</span>
      <span className="grow-text">{p.title || <span className="muted">(no title)</span>}</span>
      <StatusPill status={p.status} />
      <span className="muted">{p.author}</span>
      <span className="muted row-when"><Ago unix={p.date} /></span>
    </button>
  );
}

function rowFor(list: WpList, item: unknown): ReactNode {
  switch (list) {
    case "orders":
    case "subscriptions": {
      const o = item as OrderSummary;
      return <OrderRow key={o.id} o={o} />;
    }
    case "customers": {
      const c = item as CustomerSummary;
      return <CustomerRow key={c.id} c={c} />;
    }
    case "products": {
      const p = item as ProductSummary;
      return <ProductRow key={p.id} p={p} />;
    }
    case "actions": {
      const a = item as ActionSummary;
      return <ActionRow key={a.id} a={a} />;
    }
    case "sessions": {
      const s = item as SessionSummary;
      return <SessionRow key={s.key} s={s} />;
    }
    default: {
      const p = item as PostSummary;
      return <PostRow key={p.id} p={p} />;
    }
  }
}

function ActionHistogram({ hook }: { hook: string | null }) {
  const { data } = useWpData((s, d) => api.wpActionHistogram(s, d, hook), [hook]);
  if (!data) return null;
  return (
    <Card title={`Scheduled actions per day${hook ? ` · ${hook}` : ""}`}>
      <StackedBars days={data} labels={{ completed: "complete", processing: "pending", failed: "failed", other: "canceled" }} />
    </Card>
  );
}

function FailedByHook({ onPick }: { onPick: (hook: string) => void }) {
  const { data } = useWpData(api.wpFailedByHook, []);
  if (!data || data.length === 0) return null;
  const items = data.map((h) => ({
    id: h.hook,
    label: h.hook,
    value: h.count,
    sublabel: [h.owner, h.lastFailure ? `last ${relativeFromUnix(h.lastFailure)}` : null].filter(Boolean).join(" · "),
    onOpen: () => onPick(h.hook),
  }));
  return (
    <Card title="Failed actions by hook" aside="click a hook to list its failures">
      <BreakdownChart items={items} format={(v) => `${formatCount(v)}×`} totalLabel="failed" />
    </Card>
  );
}

export function ListScreen({ list, params }: { list: Exclude<WpList, "settings">; params?: Record<string, string> }) {
  const [status, setStatus] = useState<string | null>(params?.status ?? null);
  const [kind, setKind] = useState<string | null>(params?.kind ?? null);
  const [searchInput, setSearchInput] = useState(params?.search ?? "");
  const [offset, setOffset] = useState(0);
  const search = useDebounced(searchInput);
  const load = LOADERS[list];
  const { data, error, loading } = useWpData(
    (s, d) => load(s, d, { status, kind, search: search || null, offset, limit: PAGE }),
    [list, status, kind, search, offset],
  );

  useEffect(() => setOffset(0), [status, kind, search]);

  const kindFacets = data?.kinds ?? [];
  return (
    <div className="list-screen">
      <div className="list-tools">
        <div className="list-search">
          <Search size="1rem" />
          <input aria-label="Search list" placeholder="Search…" value={searchInput} onChange={(e) => setSearchInput(e.target.value)} />
        </div>
        {kindFacets.length > 0 && list !== "content" && (
          <select className="input" style={{ width: 220 }} aria-label={KIND_LABEL[list] ?? "Kind"} value={kind ?? ""} onChange={(e) => setKind(e.target.value || null)}>
            <option value="">All {(KIND_LABEL[list] ?? "kinds").toLowerCase()}s</option>
            {kindFacets.map((k) => (
              <option key={k.value} value={k.value}>
                {k.label} ({formatCount(k.count)})
              </option>
            ))}
          </select>
        )}
        <div className="spacer" />
        <span className="muted">{data ? `${offset + 1}–${offset + data.items.length}` : ""}</span>
        <button className="icon-btn" aria-label="Previous page" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - PAGE))}>
          <ChevronLeft size="1.077rem" />
        </button>
        <button className="icon-btn" aria-label="Next page" disabled={!data?.hasMore} onClick={() => setOffset(offset + PAGE)}>
          <ChevronRight size="1.077rem" />
        </button>
      </div>
      {list === "content" && <Chips facets={kindFacets} value={kind ?? kindFacets.find((k) => k.value === "post")?.value ?? null} onChange={setKind} allLabel="Types" />}
      <Chips facets={data?.facets ?? []} value={status} onChange={setStatus} />
      {list === "actions" && !kind && <FailedByHook onPick={(hook) => (setKind(hook), setStatus("failed"))} />}
      {list === "actions" && <ActionHistogram hook={kind} />}
      {error ? (
        <ErrorPanel message={error.message} code={error.code} />
      ) : !data ? (
        <div className="empty-state">Loading…</div>
      ) : data.items.length === 0 ? (
        <div className="empty-state">{loading ? "Loading…" : "Nothing matches these filters."}</div>
      ) : (
        <div className={`rows${loading ? " stale" : ""}`}>{data.items.map((item) => rowFor(list, item))}</div>
      )}
    </div>
  );
}
