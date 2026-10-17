/** Holding ⌥ peeks the hovered, else the focused, reference (docs/ux.md, Peek); ↵ opens it. */
import { useCallback, useEffect, useState, type CSSProperties } from "react";
import { api, toAppError, type Peek } from "../../api";
import { formatAddress, parseAddress } from "../../lib/address";
import { bySourceText } from "../../lib/provenance";
import { selectActiveTab, selectFocusedValue, useStore } from "../../state/store";
import { openTarget, type ColumnPlace } from "./open";
import { Value } from "./Value";

interface PeekTarget {
  address: string;
  via: string | null;
  by: string | null;
  rect: DOMRect | null;
  /** The column the reference is in; opening it drills there. */
  place: ColumnPlace | null;
}

const OFFSET = 6;
const WIDTH = 380;

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
      if (e.key !== "Enter") return;
      e.preventDefault();
      e.stopPropagation();
      close();
      openTarget(parseAddress(target.address), target.place);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [target, close]);
}

type Loaded = { state: "loading" } | { state: "loaded"; peek: Peek } | { state: "failed"; error: string };

const LOADING: Loaded = { state: "loading" };

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

function usePeekData(address: string): Loaded {
  const slug = parseAddress(address).connection;
  const sessionId = useStore((s) => s.sessions.find((x) => x.slug === slug)?.id ?? null);
  const dataVersion = useStore((s) => s.dataVersion);
  const [loaded, setLoaded] = useState<{ key: string; data: Loaded } | null>(null);
  const key = `${sessionId}|${dataVersion}|${address}`;
  useEffect(() => {
    if (!sessionId) return;
    let alive = true;
    cachedPeek(sessionId, address, dataVersion).then(
      (peek) => alive && setLoaded({ key, data: { state: "loaded", peek } }),
      (e) => alive && setLoaded({ key, data: { state: "failed", error: toAppError(e).message } }),
    );
    return () => {
      alive = false;
    };
  }, [key, sessionId, address, dataVersion]);
  return loaded?.key === key ? loaded.data : LOADING;
}

function position(rect: DOMRect | null): CSSProperties {
  if (!rect) return { right: 60, top: 80 };
  return { left: Math.max(8, Math.min(rect.left, window.innerWidth - WIDTH - 8)), top: rect.bottom + OFFSET };
}

function PeekCard({ target }: { target: PeekTarget }) {
  const loaded = usePeekData(target.address);
  const address = parseAddress(target.address);
  const row = address.target.kind === "row" ? address.target : null;
  const keyColumn = loaded.state === "loaded" ? loaded.peek.keyColumn : null;
  const to = row ? `${row.table}${keyColumn ? `.${keyColumn}` : ""}` : target.address;
  const footer = [target.via ? `via ${target.via} → ${to}` : `→ ${to}`, target.by].filter(Boolean).join(" · ");
  const label = `Peek ${row ? `${row.table} ${row.key}` : target.address}`;
  return (
    <aside className="peek" aria-label={label} data-stage="peek" style={{ ...position(target.rect), width: WIDTH }}>
      <div className="peek-head">
        <span className="mono">
          <span className="tn">{row?.table}</span> <span className="key">#{row?.key}</span>
        </span>
        <span className="o3">⌥ peek · ↵ open</span>
      </div>
      {loaded.state === "loading" && <div className="peek-row o3">Loading…</div>}
      {loaded.state === "failed" && <div className="peek-row o3">{loaded.error}</div>}
      {loaded.state === "loaded" &&
        loaded.peek.values.map(([name, value]) => (
          <div key={name} className="peek-row">
            <span className="o2">{name}</span>
            <Value value={value} density="grid" />
          </div>
        ))}
      <div className="peek-foot">{footer}</div>
    </aside>
  );
}

export function PeekOverlay() {
  const [target, close] = usePeekKeys();
  useOpenKey(target, close);
  return target ? <PeekCard target={target} /> : null;
}
