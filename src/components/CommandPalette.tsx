import { useEffect, useMemo, useRef, useState } from "react";
import { CornerDownLeft, Search } from "lucide-react";
import { api, type SearchHit } from "../api";
import { allCommands, bindingFor, bindingLabel, type Command } from "../commands";
import { selectActiveSession, selectWp, useStore, type RecentNav } from "../state/store";

interface Item {
  key: string;
  section: "Recent" | "Commands" | "Tables" | "Entities";
  title: string;
  subtitle?: string;
  badge?: string;
  binding?: string;
  go: () => void;
  /** ⌘↵: open the table in a new tab, or an entity's source row. */
  goAlt?: () => void;
}

type Prefix = ">" | "#" | "t:" | "@" | null;

export function parsePrefix(input: string): { prefix: Prefix; needle: string } {
  const q = input.trimStart();
  for (const prefix of [">", "t:", "#", "@"] as const) {
    if (q.startsWith(prefix)) return { prefix, needle: q.slice(prefix.length).trim() };
  }
  return { prefix: null, needle: q.trim() };
}

function score(title: string, needle: string): number {
  if (!needle) return 1;
  const t = title.toLowerCase();
  const n = needle.toLowerCase();
  if (t === n) return 4;
  if (t.startsWith(n)) return 3;
  if (t.includes(n)) return 2;
  return 0;
}

const recentTitle = (r: RecentNav) => (r.kind === "entity" ? r.title : `${r.database}.${r.table}`);

/** ⌘K: commands, tables, entities (IDs, emails, PayPal IDs…) and recent items in one place. */
export function CommandPalette() {
  const open = useStore((s) => s.paletteOpen);
  const seed = useStore((s) => s.paletteSeed);
  const session = useStore(selectActiveSession);
  const wp = useStore(selectWp);
  const mode = useStore((s) => s.mode);
  const tables = useStore((s) => (session?.database ? s.schema[session.id]?.tables[session.database] : undefined));
  const recent = useStore((s) => s.recentNav);
  const { setPaletteOpen, openEntity, openTableTab, setMode, loadTables } = useStore.getState();
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [searching, setSearching] = useState(false);
  const [index, setIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      setQuery(seed);
      setHits([]);
      setIndex(0);
      setTimeout(() => inputRef.current?.focus(), 0);
      if (session?.database && !tables) void loadTables(session.id, session.database);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const { prefix, needle } = parsePrefix(query);
  const wantEntities = !!wp && (prefix === null || prefix === "#" || prefix === "@");

  useEffect(() => {
    if (!open || !session?.database || !wantEntities || needle.length < 2) {
      setHits([]);
      return;
    }
    let alive = true;
    setSearching(true);
    const t = setTimeout(() => {
      api.wpSearch(session.id, session.database!, needle).then(
        (h) => alive && (setHits(h), setSearching(false)),
        () => alive && (setHits([]), setSearching(false)),
      );
    }, 200);
    return () => {
      alive = false;
      clearTimeout(t);
    };
  }, [needle, wantEntities, open, session?.id, session?.database]);

  const items: Item[] = useMemo(() => {
    const close = () => setPaletteOpen(false);
    const db = session?.database;
    const inMode = (c: Command) => ((c.scope === "wp") === (mode === "wp") ? 1 : 0);
    const commandItems = (prefix === null || prefix === ">" ? allCommands() : [])
      .filter((c: Command) => c.when?.() !== false)
      .map((c) => ({ c, s: score(c.title, needle) }))
      .filter(({ s }) => s > 0)
      .sort((a, b) => b.s - a.s || inMode(b.c) - inMode(a.c))
      .map(({ c }): Item => ({
        key: `cmd:${c.id}`,
        section: "Commands",
        title: c.title,
        binding: bindingLabel(bindingFor(c.id)),
        go: () => (close(), void c.run()),
      }));
    const tableItems = (prefix === null || prefix === "t:" ? (tables ?? []) : [])
      .map((t) => ({ t, s: score(t.name, needle) }))
      .filter(({ s }) => s > 0 && (needle || prefix === "t:"))
      .sort((a, b) => b.s - a.s)
      .slice(0, 40)
      .map(({ t }): Item => {
        const openIt = () => (close(), setMode("db"), db && openTableTab(db, t.name));
        return { key: `table:${t.name}`, section: "Tables", title: t.name, badge: t.isView ? "view" : "table", go: openIt, goAlt: openIt };
      });
    const entityItems = hits.map(
      (h): Item => ({
        key: `${h.entity}:${h.id}:${h.matched}`,
        section: "Entities",
        title: h.title,
        subtitle: [h.subtitle, h.matched !== "title" && h.matched !== "name" ? `matched ${h.matched}` : null].filter(Boolean).join(" · "),
        badge: h.entity,
        go: () => {
          close();
          if (h.entity === "option") useStore.getState().navigate({ screen: "list", list: "settings" });
          else openEntity(h.entity, h.id);
        },
      }),
    );
    const recentItems = (!needle && prefix === null ? recent : []).map(
      (r): Item => ({
        key: `recent:${recentTitle(r)}`,
        section: "Recent",
        title: recentTitle(r),
        badge: r.kind === "entity" ? r.entity : "table",
        go: () => {
          close();
          if (r.kind === "entity") openEntity(r.entity, r.id);
          else (setMode("db"), openTableTab(r.database, r.table));
        },
      }),
    );
    // The current view ranks first, but nothing is hidden.
    const ordered = mode === "wp" ? [recentItems, entityItems, commandItems, tableItems] : [recentItems, tableItems, commandItems, entityItems];
    return ordered.flat().slice(0, 60);
  }, [hits, needle, prefix, tables, recent, mode, session?.database, setPaletteOpen, openEntity, openTableTab, setMode]);

  if (!open) return null;
  const choose = (i: number, alt = false) => (alt ? (items[i]?.goAlt ?? items[i]?.go) : items[i]?.go)?.();
  let lastSection: Item["section"] | null = null;
  return (
    <div className="overlay palette-overlay" onMouseDown={(e) => e.target === e.currentTarget && setPaletteOpen(false)}>
      <div className="palette" role="dialog" aria-label="Command palette" data-stage="palette" data-stage-window="palette">
        <div className="palette-input">
          <Search size="1.154rem" />
          <input
            ref={inputRef}
            aria-label="Search everything"
            data-stage="palette:search"
            placeholder={wp ? "Order ID, email, PayPal ID, table, command…   > commands · t: tables · # IDs · @ emails" : "Table or command…   > commands · t: tables"}
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setIndex(0);
            }}
            onKeyDown={(e) => {
              if (e.key === "Escape") setPaletteOpen(false);
              if (e.key === "ArrowDown") (e.preventDefault(), setIndex((i) => Math.min(items.length - 1, i + 1)));
              if (e.key === "ArrowUp") (e.preventDefault(), setIndex((i) => Math.max(0, i - 1)));
              if (e.key === "Enter") (e.preventDefault(), choose(index, e.metaKey));
            }}
          />
          {searching && <span className="muted small">searching…</span>}
        </div>
        <ul className="palette-list" role="listbox">
          {items.map((it, i) => {
            const header = it.section !== lastSection ? it.section : null;
            lastSection = it.section;
            return (
              <li key={it.key} data-stage={`palette:item:${it.key}`} data-stage-list="palette:results" data-stage-label={it.title} role="option" aria-selected={i === index} className={i === index ? "active" : ""} onMouseEnter={() => setIndex(i)} onClick={() => choose(i)}>
                {header && <span className="palette-section">{header}</span>}
                {it.badge && <span className="badge">{it.badge}</span>}
                <span className="grow-text">
                  {it.title}
                  {it.subtitle && <span className="muted"> · {it.subtitle}</span>}
                </span>
                {it.binding && <kbd className="kbd-chip">{it.binding}</kbd>}
                {i === index && <CornerDownLeft size="0.923rem" className="muted" />}
              </li>
            );
          })}
          {items.length === 0 && <li className="muted">{needle.length < 2 && wantEntities ? "Type to search…" : "Nothing found."}</li>}
        </ul>
      </div>
    </div>
  );
}
