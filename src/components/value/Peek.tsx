/** Holding ⌥ peeks the hovered, else the focused, reference (docs/ux.md, Peek); ↵ opens it. */
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { api, type Address, type Peek, type PeekMatch, type Summary } from "../../api";
import { formatAddress, parseAddress } from "../../lib/address";
import { bySourceText, rowIdentity } from "../../lib/provenance";
import { useApiResult } from "../../lib/useApiResult";
import { selectActiveTab, selectFocusedValue, selectSessionBySlug, useStore } from "../../state/store";
import { openTarget, type ColumnPlace } from "./open";
import { SummaryLine } from "./SummaryLine";
import { Value } from "./Value";

export interface PeekTarget {
  address: string;
  via: string | null;
  by: string | null;
  /** The row the reference is stored in (`rowIdentity`), left out of a set the reference matches. */
  from: string | null;
  rect: DOMRect | null;
  /** The column the reference is in; opening it drills there. */
  place: ColumnPlace | null;
}

const OFFSET = 6;
const MIN_WIDTH = 320;

function placeOf(el: Element | null): ColumnPlace | null {
  const tab = selectActiveTab(useStore.getState());
  const column = el?.closest<HTMLElement>("[data-column]");
  if (tab?.kind !== "object" || !column) return null;
  return { tabId: tab.id, index: Number(column.dataset.column) };
}

function hoveredTarget(el: Element | null): PeekTarget | null {
  const ref = el?.closest<HTMLElement>("[data-ref]");
  if (!ref?.dataset.ref) return null;
  return {
    address: ref.dataset.ref,
    via: ref.dataset.via ?? null,
    by: ref.dataset.by ?? null,
    from: ref.dataset.from ?? null,
    rect: ref.getBoundingClientRect(),
    place: placeOf(ref),
  };
}

function focusedTarget(): PeekTarget | null {
  const s = useStore.getState();
  const value = selectFocusedValue(s);
  const kind = value?.kind;
  if (kind?.kind !== "reference" || kind.exists === false || kind.targets.length !== 1) return null;
  const tab = selectActiveTab(s);
  const place = tab?.kind === "object" ? { tabId: tab.id, index: tab.active } : null;
  const shown = document.querySelector(place ? `[data-column="${place.index}"] .row .focus` : ".grid-cell.selected");
  return {
    address: formatAddress(kind.targets[0]),
    via: value?.source?.column ?? null,
    by: bySourceText(kind.by),
    from: value?.source ? rowIdentity(value.source.table, value.source.row) : null,
    rect: shown?.getBoundingClientRect() ?? null,
    place,
  };
}

const overlayOpen = () => {
  const s = useStore.getState();
  return s.dialog !== null || s.paletteOpen;
};

/** Shows a peek while ⌥ is held alone; the pointer moving onto another reference peeks that one. */
function usePeekKeys(): [PeekTarget | null, () => void] {
  const [target, setTarget] = useState<PeekTarget | null>(null);
  useEffect(() => {
    let hovered: Element | null = null;
    let held = false;
    const onOver = (e: MouseEvent) => {
      hovered = e.target instanceof Element ? e.target : null;
      if (held) {
        const next = hoveredTarget(hovered);
        if (next) setTarget(next);
      }
    };
    const onDown = (e: KeyboardEvent) => {
      if (e.key !== "Alt" || e.repeat || e.metaKey || e.ctrlKey || e.shiftKey || overlayOpen()) return;
      held = true;
      setTarget(hoveredTarget(hovered) ?? focusedTarget());
    };
    const release = () => {
      held = false;
      setTarget(null);
    };
    const onUp = (e: KeyboardEvent) => e.key === "Alt" && release();
    window.addEventListener("mouseover", onOver, true);
    window.addEventListener("keydown", onDown, true);
    window.addEventListener("keyup", onUp, true);
    window.addEventListener("blur", release);
    return () => {
      window.removeEventListener("mouseover", onOver, true);
      window.removeEventListener("keydown", onDown, true);
      window.removeEventListener("keyup", onUp, true);
      window.removeEventListener("blur", release);
    };
  }, []);
  const close = useCallback(() => setTarget(null), []);
  return [target, close];
}

function useOpenKey(target: PeekTarget | null, close: () => void) {
  useEffect(() => {
    if (!target) return;
    const onKey = (e: KeyboardEvent) => {
      // ⌘↵ and ⌃↵ belong to commands (⌘⌥↵ opens the active pane in a tab while ⌥ is still held).
      if (e.key !== "Enter" || e.metaKey || e.ctrlKey) return;
      e.preventDefault();
      e.stopPropagation();
      close();
      openTarget(parseAddress(target.address), target.place);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [target, close]);
}

/** Peeks by session and address for the current `dataVersion`; a refresh or an applied write empties it. */
const cache = new Map<string, Promise<Peek>>();
let cacheVersion = -1;

function cachedPeek(sessionId: string, address: string, dataVersion: number): Promise<Peek> {
  if (dataVersion !== cacheVersion) {
    cache.clear();
    cacheVersion = dataVersion;
  }
  const key = `${sessionId}|${address}`;
  let request = cache.get(key);
  if (!request) {
    request = api.peek(sessionId, address);
    cache.set(key, request);
    request.catch(() => cache.delete(key));
  }
  return request;
}

function usePeekData(address: string) {
  const slug = parseAddress(address).connection;
  const sessionId = useStore((s) => selectSessionBySlug(s, slug)?.id ?? null);
  const dataVersion = useStore((s) => s.dataVersion);
  const key = sessionId ? JSON.stringify([sessionId, dataVersion, address]) : null;
  return useApiResult(key, () => cachedPeek(sessionId!, address, dataVersion));
}

function position(rect: DOMRect | null, width: number): CSSProperties {
  if (!rect) return { right: 60, top: 80 };
  return { left: Math.max(8, Math.min(rect.left, window.innerWidth - width - 8)), top: rect.bottom + OFFSET };
}

/** The card's rendered width, so it stays in the window however wide its values make it. */
function useCardWidth(content: unknown) {
  const ref = useRef<HTMLElement>(null);
  const [width, setWidth] = useState(MIN_WIDTH);
  useLayoutEffect(() => {
    setWidth(Math.max(MIN_WIDTH, ref.current?.offsetWidth ?? 0));
  }, [content]);
  return [ref, width] as const;
}

const MATCH_ROWS = 5;

function matchIdentity(match: PeekMatch): string | null {
  return match.address.target.kind === "row" ? rowIdentity(match.address.target.table, match.key) : null;
}

/** `wp_wc_orders #4711`, or for a set `wp_wc_orders · billing_email = ada@example.com`. */
function PeekTitle({ address }: { address: Address }) {
  const t = address.target;
  if (t.kind === "row")
    return (
      <span className="mono">
        <span className="tn">{t.table}</span> <span className="key">#{t.key}</span>
      </span>
    );
  if (t.kind !== "table") return <span className="mono">{formatAddress(address)}</span>;
  return (
    <span className="mono">
      <span className="tn">{t.table}</span>
      {t.filter.map((f) => (
        <span key={f.column} className="o2">
          {" · "}
          {f.column} = {f.value ?? "NULL"}
        </span>
      ))}
    </span>
  );
}

function matchesText(total: number, others: boolean): string {
  const other = others ? "other " : "";
  if (total === 0) return `no ${other}row matches`;
  return total === 1 ? `1 ${other}row matches` : `${total} ${other}rows match`;
}

/** The rows a set address selects, without the row the reference is stored in. */
function Matches({ data, from, onOpen }: { data: Peek; from: string | null; onOpen: (address: Address) => void }) {
  const all = data.matches?.rows ?? [];
  const rows = all.filter((m) => !from || matchIdentity(m) !== from);
  const dropped = all.length - rows.length;
  const total = (data.matches?.total ?? 0) - dropped;
  const shown = rows.slice(0, MATCH_ROWS);
  return (
    <>
      <div className="peek-row peek-count o2">{matchesText(total, dropped > 0)}</div>
      {shown.map((match) => (
        <button key={formatAddress(match.address)} className="peek-row peek-match" onClick={() => onOpen(match.address)}>
          <span className="key">#{match.address.target.kind === "row" ? match.address.target.key : ""}</span>
          <SummaryLine summary={match.summary} />
        </button>
      ))}
      {total > shown.length && <div className="peek-row o3">{total - shown.length} more · ↵ opens all</div>}
    </>
  );
}

function rowIdentityOf(data: Peek): string | null {
  const t = data.address.target;
  return t.kind === "row" && data.keyColumn ? rowIdentity(t.table, [{ column: data.keyColumn, value: t.key }]) : null;
}

/** A set whose only match is the row the reference is stored in, which the backend peeks as that row. */
const onlySelf = (data: Peek, target: PeekTarget) => target.from !== null && parseAddress(target.address).target.kind === "table" && rowIdentityOf(data) === target.from;

function PeekBody({ data, target, onOpen }: { data: Peek; target: PeekTarget; onOpen: (address: Address) => void }) {
  if (data.matches) return <Matches data={data} from={target.from} onOpen={onOpen} />;
  if (onlySelf(data, target)) return <div className="peek-row peek-count o2">{matchesText(0, true)}</div>;
  if (data.summary.values.length === 0) return <div className="peek-row o3">no values</div>;
  return <SummaryTable summary={data.summary} />;
}

function SummaryTable({ summary }: { summary: Summary }) {
  return (
    <table className="peek-table">
      <tbody>
        {summary.values.map(({ name, value }) => (
          <tr key={name}>
            <th scope="row" title={name}>
              {name}
            </th>
            <td>
              <Value value={value} density="grid" />
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export function PeekCard({ target, onClose }: { target: PeekTarget; onClose?: () => void }) {
  const loaded = usePeekData(target.address);
  const requested = parseAddress(target.address);
  const data = loaded.data;
  const [ref, width] = useCardWidth(data ?? loaded.state);
  const address = data && !data.matches && !onlySelf(data, target) ? data.address : requested;
  const row = address.target.kind === "row" ? address.target : null;
  const keyColumn = data?.keyColumn ?? null;
  const to = row ? `${row.table}${keyColumn ? `.${keyColumn}` : ""}` : requested.target.kind === "table" ? requested.target.table : target.address;
  const footer = [target.via ? `via ${target.via} → ${to}` : `→ ${to}`, target.by].filter(Boolean).join(" · ");
  const rt = requested.target;
  const label = `Peek ${rt.kind === "row" ? `${rt.table} ${rt.key}` : target.address}`;
  const open = (to: Address) => {
    onClose?.();
    openTarget(to, target.place);
  };
  return (
    <aside ref={ref} className="peek" aria-label={label} data-stage="peek" style={position(target.rect, width)}>
      <div className="peek-head">
        <PeekTitle address={address} />
        <span className="o3">⌥ peek · ↵ open</span>
      </div>
      {loaded.state === "loading" && <div className="peek-row o3">Loading…</div>}
      {loaded.state === "failed" && <div className="peek-row o3">{loaded.error.message}</div>}
      {loaded.state === "loaded" && <PeekBody data={loaded.data} target={target} onOpen={open} />}
      <div className="peek-foot">{footer}</div>
    </aside>
  );
}

export function PeekOverlay() {
  const [target, close] = usePeekKeys();
  useOpenKey(target, close);
  return target ? <PeekCard target={target} onClose={close} /> : null;
}
