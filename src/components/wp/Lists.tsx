import { useCallback, useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { AlertTriangle, ArrowDown, ArrowUp, ChevronLeft, ChevronRight, ExternalLink, Search, X } from "lucide-react";
import {
  api,
  type ActionSummary,
  type UserSummary,
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
import { selectWpRoute, useStore, type WpList } from "../../state/store";
import { ErrorPanel } from "../QueryTab";
import { DEFAULT_PANE_HEIGHT, Splitter, usePaneSize, useWindowSize } from "../Splitter";
import { BreakdownChart } from "./BreakdownChart";
import { StackedBars } from "./charts";
import { Ago, Avatar, Card, StatusPill, Thumb } from "./ui";
import { useWpData } from "./useWpData";
import { EntityScreen } from "./views/EntityScreen";

const PAGE = 50;
/** Below this window width the detail opens as a full page instead of a side pane. */
export const SPLIT_MIN_WIDTH = 1400;
const DEFAULT_SPLIT = 560;

type Loader = (sessionId: string, database: string, q: ListQuery) => Promise<Page<unknown>>;

const LOADERS: Record<Exclude<WpList, "settings">, Loader> = {
  orders: (s, d, q) => api.wpOrders(s, d, "shop_order", q),
  subscriptions: (s, d, q) => api.wpOrders(s, d, "shop_subscription", q),
  users: api.wpUsers,
  products: api.wpProducts,
  actions: api.wpActions,
  sessions: api.wpSessions,
  content: api.wpContent,
};

const KIND_LABEL: Partial<Record<WpList, string>> = {
  orders: "Gateway",
  subscriptions: "Gateway",
  users: "Role",
  products: "Type",
  actions: "Hook",
  content: "Type",
};

interface Column {
  label: string;
  className: string;
  /** Sort key accepted by the backend; absent = not sortable. */
  sort?: string;
}

/** Header columns mirror the row layout classes so labels line up with values. */
const COLUMNS: Record<Exclude<WpList, "settings">, Column[]> = {
  orders: [
    { label: "Order", className: "row-id", sort: "id" },
    { label: "Customer", className: "grow-text" },
    { label: "Status", className: "col-status", sort: "status" },
    { label: "Gateway", className: "row-gateway" },
    { label: "Items", className: "row-items" },
    { label: "Total", className: "row-total", sort: "total" },
    { label: "Created", className: "row-when", sort: "created" },
  ],
  subscriptions: [
    { label: "Subscription", className: "row-id", sort: "id" },
    { label: "Customer", className: "grow-text" },
    { label: "Status", className: "col-status", sort: "status" },
    { label: "Gateway", className: "row-gateway" },
    { label: "Items", className: "row-items" },
    { label: "Total", className: "row-total", sort: "total" },
    { label: "Created", className: "row-when", sort: "created" },
  ],
  users: [
    { label: "", className: "col-avatar" },
    { label: "User", className: "grow-text", sort: "name" },
    { label: "Roles", className: "row-roles" },
    { label: "Orders", className: "row-items" },
    { label: "Spent", className: "row-total" },
    { label: "Registered", className: "row-when", sort: "registered" },
  ],
  products: [
    { label: "", className: "col-avatar" },
    { label: "Product", className: "grow-text", sort: "name" },
    { label: "Stock", className: "col-status" },
    { label: "Sold", className: "row-items", sort: "sales" },
    { label: "Price", className: "row-total", sort: "price" },
  ],
  actions: [
    { label: "Status", className: "col-status", sort: "status" },
    { label: "Hook", className: "grow-text", sort: "hook" },
    { label: "Group", className: "row-gateway" },
    { label: "Tries", className: "row-items", sort: "attempts" },
    { label: "Scheduled", className: "row-when", sort: "scheduled" },
  ],
  sessions: [
    { label: "Session", className: "grow-text" },
    { label: "Customer", className: "row-gateway" },
    { label: "Cart", className: "row-items" },
    { label: "Total", className: "row-total" },
    { label: "Expires", className: "row-when" },
  ],
  content: [
    { label: "ID", className: "row-id" },
    { label: "Title", className: "grow-text", sort: "title" },
    { label: "Status", className: "col-status", sort: "status" },
    { label: "Author", className: "row-gateway" },
    { label: "Date", className: "row-when", sort: "date" },
  ],
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

/** What a row opens: the entity kind and id. */
export function rowTarget(list: WpList, item: unknown): { entity: string; id: string } {
  switch (list) {
    case "orders":
    case "subscriptions":
      return { entity: "order", id: (item as OrderSummary).id };
    case "users":
      return { entity: "user", id: (item as UserSummary).id };
    case "actions":
      return { entity: "action", id: (item as ActionSummary).id };
    case "sessions":
      return { entity: "session", id: (item as SessionSummary).key };
    default:
      return { entity: "post", id: (item as PostSummary).id };
  }
}

function Row({ stage, active, onOpen, children, title }: { stage: string; active: boolean; onOpen: () => void; children: ReactNode; title?: string }) {
  return (
    <button data-stage={stage} className={`row-link${active ? " selected" : ""}`} role="row" aria-selected={active} data-active={active || undefined} onClick={onOpen} title={title}>
      {children}
    </button>
  );
}

function OrderCells({ o }: { o: OrderSummary }) {
  return (
    <>
      <span className="row-id">#{o.id}</span>
      <span className="grow-text">
        <span>{o.name || o.email || <span className="badge">Guest</span>}</span>
        {o.name && o.email && <span className="muted"> · {o.email}</span>}
      </span>
      <span className="col-status"><StatusPill status={o.status} /></span>
      <span className="row-gateway mono" title={o.gatewayTitle ?? undefined}>{o.gateway ?? ""}</span>
      <span className="muted row-items">{o.itemCount} {o.itemCount === 1 ? "item" : "items"}</span>
      <span className="money row-total">{o.total}</span>
      <span className="muted row-when"><Ago unix={o.created} /></span>
    </>
  );
}

function UserCells({ c }: { c: UserSummary }) {
  return (
    <>
      <span className="col-avatar"><Avatar name={c.name || c.login} src={c.avatar} /></span>
      <span className="grow-text">
        <span className="mono">#{c.id} {c.login}</span>
        <span className="muted"> · {[c.name, c.email].filter(Boolean).join(" · ")}</span>
      </span>
      <span className="row-roles">{c.roles.map((r) => <span key={r} className="badge">{r}</span>)}</span>
      <span className="muted row-items">{c.orderCount} {c.orderCount === 1 ? "order" : "orders"}</span>
      <span className="money row-total">{c.totalSpent ?? ""}</span>
      <span className="muted row-when"><Ago unix={c.registered} /></span>
    </>
  );
}

function ProductCells({ p }: { p: ProductSummary }) {
  return (
    <>
      <span className="col-avatar"><Thumb src={p.image} /></span>
      <span className="grow-text">
        {p.name}
        {p.sku && <span className="muted mono"> · {p.sku}</span>}
        {p.kind && <span className="badge" style={{ marginLeft: "0.462rem" }}>{p.kind}</span>}
        {p.variations > 0 && <span className="muted small"> · {p.variations} variations</span>}
        {p.status !== "publish" && <span style={{ marginLeft: "0.462rem" }}><StatusPill status={p.status} /></span>}
      </span>
      <span className="col-status">{p.stockStatus && <StatusPill status={p.stockStatus} label={`${p.stockStatus}${p.stock ? ` · ${p.stock}` : ""}`} />}</span>
      <span className="muted row-items">{p.totalSales} sold</span>
      <span className="money row-total">{p.price}</span>
    </>
  );
}

function ActionCells({ a }: { a: ActionSummary }) {
  return (
    <>
      <span className="col-status"><StatusPill status={a.status} /></span>
      <span className="grow-text">
        <span className="mono">{a.hook}</span>
        {a.args && <span className="muted mono"> {shorten(a.args, 40, 8)}</span>}
        {a.status === "failed" && a.message && (
          <span className="row-sub tone-text-red"><AlertTriangle size="0.846rem" /> {shorten(a.message, 90, 10)}</span>
        )}
      </span>
      <span className="row-gateway">{a.group && <span className="badge">{a.group}</span>}</span>
      <span className="muted row-items">{a.attempts}×</span>
      <span className="muted row-when"><Ago unix={a.scheduled} /></span>
    </>
  );
}

function SessionCells({ s }: { s: SessionSummary }) {
  const expired = s.expires != null && s.expires < Date.now() / 1000;
  return (
    <>
      <span className="mono grow-text">{shorten(s.key, 20, 4)}</span>
      <span className="row-gateway">{s.customerId ? (s.email ?? `User #${s.customerId}`) : <span className="badge">Guest</span>}</span>
      <span className="muted row-items">{s.items} in cart</span>
      <span className="money row-total">{s.total}</span>
      <span className={`row-when ${expired ? "tone-text-red" : "muted"}`}>
        {expired ? "expired " : "expires "}
        <Ago unix={s.expires} />
      </span>
    </>
  );
}

function PostCells({ p }: { p: PostSummary }) {
  return (
    <>
      <span className="row-id">#{p.id}</span>
      <span className="grow-text">{p.title || <span className="muted">(no title)</span>}</span>
      <span className="col-status"><StatusPill status={p.status} /></span>
      <span className="muted row-gateway">{p.author}</span>
      <span className="muted row-when"><Ago unix={p.date} /></span>
    </>
  );
}

function cellsFor(list: WpList, item: unknown): { key: string; node: ReactNode; title?: string } {
  switch (list) {
    case "orders":
    case "subscriptions": {
      const o = item as OrderSummary;
      return { key: o.id, node: <OrderCells o={o} /> };
    }
    case "users": {
      const c = item as UserSummary;
      return { key: c.id, node: <UserCells c={c} /> };
    }
    case "products": {
      const p = item as ProductSummary;
      return { key: p.id, node: <ProductCells p={p} /> };
    }
    case "actions": {
      const a = item as ActionSummary;
      return { key: a.id, node: <ActionCells a={a} />, title: a.message ?? undefined };
    }
    case "sessions": {
      const s = item as SessionSummary;
      return { key: s.key, node: <SessionCells s={s} /> };
    }
    default: {
      const p = item as PostSummary;
      return { key: p.id, node: <PostCells p={p} /> };
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
    sublabel: [h.lastFailure ? `last ${relativeFromUnix(h.lastFailure)}` : null].filter(Boolean).join(" · "),
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
  const [sort, setSort] = useState<{ key: string; descending: boolean } | null>(null);
  const [offset, setOffset] = useState(0);
  const search = useDebounced(searchInput);
  const load = LOADERS[list];
  const { data, error, loading } = useWpData(
    (s, d) => load(s, d, { status, kind, search: search || null, offset, limit: PAGE, sort: sort?.key ?? null, descending: sort?.descending ?? null }),
    [list, status, kind, search, offset, sort?.key, sort?.descending],
  );

  const route = useStore(selectWpRoute);
  const { replaceRoute, openEntity } = useStore.getState();
  const bottom = useStore((s) => s.settings?.inspectorDock === "bottom");
  const { width, height } = useWindowSize();
  const side = usePaneSize("splitWidth", DEFAULT_SPLIT, 360, width - 480);
  const below = usePaneSize("paneHeight", DEFAULT_PANE_HEIGHT, 160, height - 260);
  const pane = bottom ? below : side;
  const split = bottom || width >= SPLIT_MIN_WIDTH;
  const selected = route.screen === "list" && route.list === list ? (route.selected ?? null) : null;
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => setOffset(0), [status, kind, search, sort]);

  const items = data?.items ?? [];
  const select = useCallback(
    (item: unknown) => {
      const target = rowTarget(list, item);
      if (!split) return openEntity(target.entity, target.id);
      if (route.screen === "list") replaceRoute({ ...route, selected: target });
    },
    [list, split, route, replaceRoute, openEntity],
  );

  const onKey = (e: KeyboardEvent) => {
    if (!items.length) return;
    const index = selected ? items.findIndex((i) => rowTarget(list, i).id === selected.id) : -1;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = items[Math.min(items.length - 1, Math.max(0, index + (e.key === "ArrowDown" ? 1 : -1)))];
      select(next);
      listRef.current?.querySelector<HTMLElement>(`[data-active]`)?.scrollIntoView?.({ block: "nearest" });
    } else if (e.key === "Enter" && selected) {
      e.preventDefault();
      openEntity(selected.entity, selected.id);
    }
  };

  const toggleSort = (key: string) =>
    setSort((s) => (s?.key === key ? (s.descending ? { key, descending: false } : null) : { key, descending: true }));

  const kindFacets = data?.kinds ?? [];
  const columns = COLUMNS[list];
  const listPane = (
    <div className="list-screen">
      <div className="list-tools">
        <div className="list-search">
          <Search size="1rem" />
          <input data-stage="list:search" aria-label="Search list" placeholder="Search…" value={searchInput} onChange={(e) => setSearchInput(e.target.value)} />
        </div>
        {kindFacets.length > 0 && list !== "content" && (
          <select className="input" style={{ width: "16.923rem" }} aria-label={KIND_LABEL[list] ?? "Kind"} value={kind ?? ""} onChange={(e) => setKind(e.target.value || null)}>
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
        <button data-stage="list:previous-page" className="icon-btn" aria-label="Previous page" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - PAGE))}>
          <ChevronLeft size="1.077rem" />
        </button>
        <button data-stage="list:next-page" className="icon-btn" aria-label="Next page" disabled={!data?.hasMore} onClick={() => setOffset(offset + PAGE)}>
          <ChevronRight size="1.077rem" />
        </button>
      </div>
      {list === "content" && <Chips facets={kindFacets} value={kind ?? kindFacets.find((k) => k.value === "post")?.value ?? null} onChange={setKind} allLabel="Types" />}
      <Chips facets={data?.facets ?? []} value={status} onChange={setStatus} />
      {list === "actions" && !kind && !selected && <FailedByHook onPick={(hook) => (setKind(hook), setStatus("failed"))} />}
      {list === "actions" && !selected && <ActionHistogram hook={kind} />}
      {error ? (
        <ErrorPanel message={error.message} code={error.code} />
      ) : !data ? (
        <div className="empty-state">Loading…</div>
      ) : data.items.length === 0 ? (
        <div className="empty-state">{loading ? "Loading…" : "Nothing matches these filters."}</div>
      ) : (
        <div className={`rows${loading ? " stale" : ""}`} role="grid" aria-label={`${list} list`} tabIndex={0} ref={listRef} onKeyDown={onKey}>
          <div className="list-head" role="row">
            {columns.map((c) => {
              const active = sort?.key === c.sort;
              return c.sort ? (
                <button key={c.label || c.className} role="columnheader" aria-sort={active ? (sort!.descending ? "descending" : "ascending") : "none"} className={`${c.className} sortable`} onClick={() => toggleSort(c.sort!)}>
                  {c.label}
                  {active && (sort!.descending ? <ArrowDown size="0.769rem" /> : <ArrowUp size="0.769rem" />)}
                </button>
              ) : (
                <span key={c.label || c.className} role="columnheader" className={c.className}>{c.label}</span>
              );
            })}
          </div>
          {data.items.map((item) => {
            const { key, node, title } = cellsFor(list, item);
            const target = rowTarget(list, item);
            return (
              <Row key={key} stage={`${target.entity}:${target.id}`} active={selected?.id === target.id} onOpen={() => select(item)} title={title}>
                {node}
              </Row>
            );
          })}
        </div>
      )}
    </div>
  );

  if (!split || !selected) return listPane;
  return (
    <div className={`list-split${bottom ? " dock-bottom" : ""}`} style={bottom ? { gridTemplateRows: `minmax(0, 1fr) auto ${pane.size}px` } : { gridTemplateColumns: `minmax(0, 1fr) auto ${pane.size}px` }}>
      {listPane}
      <Splitter axis={bottom ? "y" : "x"} paneAfter label="Resize detail pane" {...pane} />
      <aside className="detail-pane" aria-label="Detail">
        <div className="detail-head">
          <span className="muted small">{selected.entity} #{selected.id}</span>
          <div className="spacer" />
          <button className="btn tiny" onClick={() => openEntity(selected.entity, selected.id)} title="Open as a full page (↵)">
            <ExternalLink size="0.846rem" /> Open page
          </button>
          <button className="icon-btn tiny" aria-label="Close detail" onClick={() => route.screen === "list" && replaceRoute({ ...route, selected: undefined })}>
            <X size="0.846rem" />
          </button>
        </div>
        <div className="detail-body">
          <EntityScreen key={`${selected.entity}:${selected.id}`} entity={selected.entity} id={selected.id} embedded />
        </div>
      </aside>
    </div>
  );
}
