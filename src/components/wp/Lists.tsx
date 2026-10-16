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
import { formatCount, relativeTime } from "../../lib/format";
import { shorten } from "../../lib/wpFormat";
import { selectWpRoute, useStore, type WpList } from "../../state/store";
import { ErrorPanel } from "../ErrorPanel";
import { DEFAULT_PANE_HEIGHT, Splitter, usePaneSize, useWindowSize } from "../Splitter";
import { BreakdownChart } from "./BreakdownChart";
import { StackedBars } from "./charts";
import { Ago, Avatar, Card, StatusPill, Thumb } from "./ui";
import { useWpData } from "./useWpData";
import { EntityScreen } from "./views/EntityScreen";

const PAGE = 50;
const SEARCH_DEBOUNCE_MS = 250;
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

/** The kind filter's label per list, singular and plural. */
const KIND_LABEL: Partial<Record<WpList, { one: string; many: string }>> = {
  orders: { one: "Gateway", many: "gateways" },
  subscriptions: { one: "Gateway", many: "gateways" },
  users: { one: "Role", many: "roles" },
  products: { one: "Type", many: "types" },
  actions: { one: "Hook", many: "hooks" },
  content: { one: "Type", many: "types" },
};
const DEFAULT_KIND_LABEL = { one: "Kind", many: "kinds" };

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

function Row({ stage, list, active, onOpen, children, title }: { stage: string; list: string; active: boolean; onOpen: () => void; children: ReactNode; title?: string }) {
  return (
    <button data-stage={stage} data-stage-list={list} className={`row-link${active ? " selected" : ""}`} role="row" aria-selected={active} data-active={active || undefined} onClick={onOpen} title={title}>
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
  const { data } = useWpData([hook], (s, d) => api.wpActionHistogram(s, d, hook));
  if (!data) return null;
  return (
    <Card title={`Scheduled actions per day${hook ? ` · ${hook}` : ""}`}>
      <StackedBars days={data.days} labels={data.labels} />
    </Card>
  );
}

function FailedByHook({ onPick }: { onPick: (hook: string) => void }) {
  const { data } = useWpData([], api.wpFailedByHook);
  if (!data || data.length === 0) return null;
  const items = data.map((h) => ({
    id: h.hook,
    label: h.hook,
    value: h.count,
    sublabel: [h.lastFailure ? `last ${relativeTime(h.lastFailure)}` : null].filter(Boolean).join(" · "),
    onOpen: () => onPick(h.hook),
  }));
  return (
    <Card title="Failed actions by hook" aside="click a hook to list its failures">
      <BreakdownChart items={items} format={(v) => `${formatCount(v)}×`} totalLabel="failed" />
    </Card>
  );
}

type Filters = Record<string, string | undefined>;

/** Filters live in `route.params`, so back and forward restore them. */
function useListFilters(list: WpList, params: Record<string, string> = {}) {
  const sort = params.sort ? { key: params.sort, descending: params.order !== "asc" } : null;
  const filters = {
    status: params.status ?? null,
    kind: params.kind ?? null,
    search: params.search ?? "",
    offset: Number(params.offset ?? 0),
    sort,
  };
  /** Any filter change except paging starts at the first page, in the same route update. */
  const update = (patch: Filters) => {
    const { wpNav, replaceRoute } = useStore.getState();
    const route = wpNav.stack[wpNav.index];
    if (route?.screen !== "list" || route.list !== list) return;
    const next: Filters = { ...route.params, offset: undefined, ...patch };
    const kept = Object.entries(next).filter((entry): entry is [string, string] => entry[1] != null && entry[1] !== "");
    replaceRoute({ ...route, params: Object.fromEntries(kept) });
  };
  return { filters, update };
}

/** Clicking a column header cycles its sort: descending, ascending, then unsorted. */
function nextSort(current: { key: string; descending: boolean } | null, key: string): Filters {
  if (current?.key !== key) return { sort: key, order: "desc" };
  if (current.descending) return { sort: key, order: "asc" };
  return { sort: undefined, order: undefined };
}

/** Runs only the latest scheduled callback once `ms` pass quietly; a pending one is dropped on unmount. */
function useDebouncedCallback(ms: number) {
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  return useCallback(
    (run: () => void) => {
      clearTimeout(timer.current);
      timer.current = setTimeout(run, ms);
    },
    [ms],
  );
}

export function ListScreen({ list, params }: { list: Exclude<WpList, "settings">; params?: Record<string, string> }) {
  const { filters, update } = useListFilters(list, params);
  const { status, kind, search, offset, sort } = filters;
  const [searchInput, setSearchInput] = useState(search);
  const scheduleSearch = useDebouncedCallback(SEARCH_DEBOUNCE_MS);
  const load = LOADERS[list];
  const { data, error, loading } = useWpData([list, status, kind, search, offset, sort?.key, sort?.descending], (s, d) =>
    load(s, d, { status, kind, search: search || null, offset, limit: PAGE, sort: sort?.key ?? null, descending: sort?.descending ?? null }),
  );

  const route = useStore(selectWpRoute);
  const { replaceRoute, openEntity } = useStore.getState();
  const bottom = useStore((s) => s.settings?.inspectorDock === "bottom");
  const { width, height } = useWindowSize();
  const side = usePaneSize("detailPaneWidth", DEFAULT_SPLIT, 360, width - 480);
  const below = usePaneSize("bottomPaneHeight", DEFAULT_PANE_HEIGHT, 160, height - 260);
  const pane = bottom ? below : side;
  const split = bottom || width >= SPLIT_MIN_WIDTH;
  const selected = route.screen === "list" && route.list === list ? (route.selected ?? null) : null;
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    listRef.current?.querySelector<HTMLElement>("[data-active]")?.scrollIntoView?.({ block: "nearest" });
  }, [selected?.id]);

  const items = data?.items ?? [];
  const select = useCallback(
    (item: unknown) => {
      const target = rowTarget(list, item);
      if (!split) return openEntity(target.entity, target.id);
      if (route.screen === "list") replaceRoute({ ...route, selected: target });
    },
    [list, split, route, replaceRoute, openEntity],
  );

  /** Without a selection, the first step lands on the first row. */
  const moveSelection = (step: number) => {
    const index = selected ? items.findIndex((i) => rowTarget(list, i).id === selected.id) : -1;
    select(items[Math.min(items.length - 1, Math.max(0, index + step))]);
  };

  const onKey = (e: KeyboardEvent) => {
    if (!items.length) return;
    if (e.key === "Enter" && selected) {
      e.preventDefault();
      openEntity(selected.entity, selected.id);
      return;
    }
    // Arrows move the side pane's selection; without the pane a row opens as a full page.
    if (!split || (e.key !== "ArrowDown" && e.key !== "ArrowUp")) return;
    e.preventDefault();
    moveSelection(e.key === "ArrowDown" ? 1 : -1);
  };

  const toggleSort = (key: string) => update(nextSort(sort, key));
  const setStatus = (value: string | null) => update({ status: value ?? undefined });
  const setKind = (value: string | null) => update({ kind: value ?? undefined });
  /** The route takes the search once typing pauses; a route change reaches the input by remounting. */
  const typeSearch = (value: string) => {
    setSearchInput(value);
    scheduleSearch(() => update({ search: value }));
  };
  const setOffset = (value: number) => update({ offset: value > 0 ? String(value) : undefined });

  const kindFacets = data?.kinds ?? [];
  const columns = COLUMNS[list];
  const kindLabel = KIND_LABEL[list] ?? DEFAULT_KIND_LABEL;
  const listPane = (
    <div className="list-screen">
      <div className="list-tools">
        <div className="list-search">
          <Search size="1rem" />
          <input data-stage="list:search" aria-label="Search list" placeholder="Search…" value={searchInput} onChange={(e) => typeSearch(e.target.value)} />
        </div>
        {kindFacets.length > 0 && list !== "content" && (
          <select className="input" style={{ width: "16.923rem" }} aria-label={kindLabel.one} value={kind ?? ""} onChange={(e) => setKind(e.target.value || null)}>
            <option value="">All {kindLabel.many}</option>
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
      {list === "content" && <Chips facets={kindFacets} value={kind} onChange={setKind} allLabel={`All ${kindLabel.many}`} />}
      <Chips facets={data?.facets ?? []} value={status} onChange={setStatus} />
      {list === "actions" && !kind && !selected && <FailedByHook onPick={(hook) => update({ kind: hook, status: "failed" })} />}
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
              <Row key={key} stage={`${target.entity}:${target.id}`} list={`wp:${list}`} active={selected?.id === target.id} onOpen={() => select(item)} title={title}>
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
