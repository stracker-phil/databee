import { useEffect, useMemo, useRef, useState } from "react";
import { CornerDownLeft, Search } from "lucide-react";
import { api, type SearchHit } from "../../api";
import { selectActiveSession, useStore, type WpRoute } from "../../state/store";

interface Item {
  key: string;
  title: string;
  subtitle?: string;
  badge: string;
  go: () => void;
}

const SCREENS: { title: string; route: WpRoute }[] = [
  { title: "Dashboard", route: { screen: "dashboard" } },
  { title: "Orders", route: { screen: "list", list: "orders" } },
  { title: "Failed orders", route: { screen: "list", list: "orders", params: { status: "wc-failed" } } },
  { title: "Subscriptions", route: { screen: "list", list: "subscriptions" } },
  { title: "Customers", route: { screen: "list", list: "customers" } },
  { title: "Products", route: { screen: "list", list: "products" } },
  { title: "Scheduled actions", route: { screen: "list", list: "actions" } },
  { title: "Failed actions", route: { screen: "list", list: "actions", params: { status: "failed" } } },
  { title: "Sessions", route: { screen: "list", list: "sessions" } },
  { title: "Content", route: { screen: "list", list: "content" } },
  { title: "Settings & options", route: { screen: "list", list: "settings" } },
  { title: "Snapshots & diff", route: { screen: "snapshots" } },
];

/** ⌘K: paste anything (IDs, emails, PayPal/transaction IDs, SKUs) or jump to a screen. */
export function Omnisearch() {
  const open = useStore((s) => s.paletteOpen);
  const session = useStore(selectActiveSession);
  const { setPaletteOpen, navigate, openEntity } = useStore.getState();
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [searching, setSearching] = useState(false);
  const [index, setIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      setQuery("");
      setHits([]);
      setIndex(0);
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [open]);

  useEffect(() => {
    const q = query.trim();
    if (!open || !session?.database || q.length < 2) {
      setHits([]);
      return;
    }
    let alive = true;
    setSearching(true);
    const t = setTimeout(() => {
      api.wpSearch(session.id, session.database!, q).then(
        (h) => alive && (setHits(h), setSearching(false)),
        () => alive && (setHits([]), setSearching(false)),
      );
    }, 200);
    return () => {
      alive = false;
      clearTimeout(t);
    };
  }, [query, open, session?.id, session?.database]);

  const items: Item[] = useMemo(() => {
    const close = () => setPaletteOpen(false);
    const q = query.trim().toLowerCase();
    const screens = SCREENS.filter((s) => !q || s.title.toLowerCase().includes(q)).map((s) => ({
      key: `screen:${s.title}`,
      title: s.title,
      badge: "screen",
      go: () => (close(), navigate(s.route)),
    }));
    const found = hits.map((h) => ({
      key: `${h.entity}:${h.id}:${h.matched}`,
      title: h.title,
      subtitle: [h.subtitle, h.matched !== "title" && h.matched !== "name" ? `matched ${h.matched}` : null].filter(Boolean).join(" · "),
      badge: h.entity === "post" ? "post" : h.entity,
      go: () => {
        close();
        if (h.entity === "option") navigate({ screen: "list", list: "settings" });
        else openEntity(h.entity, h.id);
      },
    }));
    return [...found, ...screens].slice(0, 30);
  }, [hits, query, navigate, openEntity, setPaletteOpen]);

  if (!open) return null;
  const choose = (i: number) => items[i]?.go();
  return (
    <div className="overlay palette-overlay" onMouseDown={(e) => e.target === e.currentTarget && setPaletteOpen(false)}>
      <div className="palette" role="dialog" aria-label="Search">
        <div className="palette-input">
          <Search size="1.154rem" />
          <input
            ref={inputRef}
            aria-label="Search everything"
            placeholder="Order ID, email, PayPal order or transaction ID, SKU, name…"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setIndex(0);
            }}
            onKeyDown={(e) => {
              if (e.key === "Escape") setPaletteOpen(false);
              if (e.key === "ArrowDown") (e.preventDefault(), setIndex((i) => Math.min(items.length - 1, i + 1)));
              if (e.key === "ArrowUp") (e.preventDefault(), setIndex((i) => Math.max(0, i - 1)));
              if (e.key === "Enter") (e.preventDefault(), choose(index));
            }}
          />
          {searching && <span className="muted small">searching…</span>}
        </div>
        <ul className="palette-list" role="listbox">
          {items.map((it, i) => (
            <li key={it.key} role="option" aria-selected={i === index} className={i === index ? "active" : ""} onMouseEnter={() => setIndex(i)} onClick={() => choose(i)}>
              <span className="badge">{it.badge}</span>
              <span className="grow-text">
                {it.title}
                {it.subtitle && <span className="muted"> · {it.subtitle}</span>}
              </span>
              {i === index && <CornerDownLeft size="0.923rem" className="muted" />}
            </li>
          ))}
          {items.length === 0 && <li className="muted">Nothing found.</li>}
        </ul>
      </div>
    </div>
  );
}
