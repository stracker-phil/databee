import { useEffect, useMemo } from "react";
import type { PaletteHit, SavedEntry, TableInfo } from "../../api";
import { allCommands, bindingFor, bindingLabel, type Command } from "../../commands";
import { AddressError, formatAddress, parseAddress } from "../../lib/address";
import { projectKey } from "../../state/slices/project";
import { savedOriginText } from "../../state/slices/saved";
import { selectActiveSession, selectActiveTab, useStore, type RecentNav } from "../../state/store";
import { objectActionCommands } from "../object/actions";
import { parsePrefix, score, searchesRows, type Prefix } from "./parse";
import { useRowHits } from "./useRowHits";
import { useSwitchItems } from "./useSwitchItems";

export interface Item {
  key: string;
  /** Recent, a hit's section (`objects with id 4711`), Address, Tables, Commands, Connections or Databases. */
  section: string;
  title: string;
  subtitle?: string;
  badge?: string;
  /** Secondary text in --o2, e.g. where a saved query is stored. */
  note?: string;
  binding?: string;
  hit?: PaletteHit;
  /** The own accent of the database a connection or database item opens, shown as a dot. */
  accent?: string;
  /** The table a table item opens, for its icon. */
  table?: { database: string; table: string };
  go: () => void;
  /** ⌘↵: tables and addresses open in a kept tab, row hits in a new tab. */
  goAlt?: () => void;
}

const MAX_ITEMS = 80;
const MAX_TABLES = 40;

const recentTitle = (r: RecentNav) => (r.kind === "entity" ? r.title : `${r.database}.${r.table}`);

const close = () => useStore.getState().setPaletteOpen(false);

/** Enter: the next column of the active object tab, else an object tab; `newTab` always opens a tab. */
function openHit(hit: PaletteHit, newTab: boolean) {
  close();
  const s = useStore.getState();
  if (hit.address.target.kind !== "row") return void s.openAddress(formatAddress(hit.address), { preview: !newTab });
  const tab = selectActiveTab(s);
  if (!newTab && tab?.kind === "object") return s.drill(tab.id, tab.active, hit.address);
  s.openObject(hit.address);
}

/** `wp_wc_orders #4711`, for Stage and screen readers. */
function hitTitle(hit: PaletteHit): string {
  const target = hit.address.target;
  return target.kind === "row" ? `${hit.source} #${target.key}` : `${hit.source} ${formatAddress(hit.address)}`;
}

function hitItems(hits: PaletteHit[]): Item[] {
  return hits.map((hit) => ({
    key: `hit:${formatAddress(hit.address)}`,
    section: hit.section,
    title: hitTitle(hit),
    hit,
    go: () => openHit(hit, false),
    goAlt: () => openHit(hit, true),
  }));
}

function addressItems(prefix: Prefix, needle: string): Item[] {
  if (prefix !== "databee://") return [];
  try {
    const address = parseAddress(needle);
    const text = formatAddress(address);
    const open = (preview: boolean) => {
      close();
      void useStore.getState().openAddress(text, { preview });
    };
    return [{ key: `address:${text}`, section: "Address", title: text, badge: "open", go: () => open(true), goAlt: () => open(false) }];
  } catch (e) {
    if (e instanceof AddressError) return [];
    throw e;
  }
}

function commandItems(prefix: Prefix, needle: string): Item[] {
  if (prefix !== null && prefix !== ">") return [];
  return [...allCommands(), ...objectActionCommands()]
    .filter((c: Command) => c.when?.() !== false)
    .map((c) => ({ c, s: score(c.title, needle) }))
    .filter(({ s }) => s > 0)
    .sort((a, b) => b.s - a.s)
    .map(({ c }): Item => ({ key: `cmd:${c.id}`, section: "Commands", title: c.title, binding: bindingLabel(bindingFor(c.id)), go: () => {
        close();
        void c.run();
      },
    }));
}

function savedItems(entries: SavedEntry[], sessionId: string | null, database: string | null, prefix: Prefix, needle: string): Item[] {
  if (prefix !== "q:" || !sessionId || !database) return [];
  return entries
    .map((entry) => ({ entry, s: score(entry.title, needle) || score(entry.id, needle) }))
    .filter(({ s }) => s > 0)
    .sort((a, b) => b.s - a.s)
    .map(({ entry }): Item => ({
      key: `saved:${savedOriginText(entry.origin)}:${entry.id}`,
      section: "Saved queries",
      title: entry.title,
      note: savedOriginText(entry.origin),
      badge: entry.pin ? "pinned" : undefined,
      go: () => {
        close();
        useStore.getState().openSavedQuery(sessionId, database, entry);
      },
    }));
}

function recentItems(recent: RecentNav[], prefix: Prefix, needle: string): Item[] {
  if (needle || prefix !== null) return [];
  const { openEntity, openTableTab } = useStore.getState();
  return recent.map((r) => ({
    key: `recent:${recentTitle(r)}`,
    section: "Recent",
    title: recentTitle(r),
    badge: r.kind === "entity" ? r.entity : "table",
    table: r.kind === "entity" ? undefined : { database: r.database, table: r.table },
    go: () => {
      close();
      if (r.kind === "entity") void openEntity(r.entity, r.id);
      else openTableTab(r.database, r.table);
    },
  }));
}

function tableItems(tables: TableInfo[], database: string | null, prefix: Prefix, needle: string): Item[] {
  if (prefix !== null && prefix !== "t:") return [];
  const { openTableTab } = useStore.getState();
  return tables
    .map((t) => ({ t, s: score(t.name, needle) }))
    .filter(({ s }) => s > 0 && (needle || prefix === "t:"))
    .sort((a, b) => b.s - a.s)
    .slice(0, MAX_TABLES)
    .map(({ t }): Item => {
      const open = (preview: boolean) => {
        close();
        if (database) openTableTab(database, t.name, undefined, undefined, { preview });
      };
      return { key: `table:${t.name}`, section: "Tables", title: t.name, badge: t.isView ? "view" : "table", table: database ? { database, table: t.name } : undefined, go: () => open(true), goAlt: () => open(false) };
    });
}

/** The palette's results for `query` in the order Recent, row hits, saved queries, Connections and Databases (`c:`), Tables, Commands. */
export function usePaletteItems(query: string) {
  const session = useStore(selectActiveSession);
  const database = session?.database ?? null;
  const tables = useStore((s) => (session && database ? s.schema[session.id]?.tables[database] : undefined));
  const info = useStore((s) => (session && database ? s.rulesInfo[projectKey(session.id, database)] : undefined));
  const saved = useStore((s) => (session && database ? s.savedQueries[projectKey(session.id, database)] : undefined));
  const recent = useStore((s) => s.recentNav);

  // Per opening (the palette is remounted each time it opens) and per database.
  const sessionId = session?.id;
  useEffect(() => {
    if (!sessionId || !database) return;
    const s = useStore.getState();
    if (!s.schema[sessionId]?.tables[database]) void s.loadTables(sessionId, database);
    if (!s.rulesInfo[projectKey(sessionId, database)]) void s.loadRulesInfo(sessionId, database);
    // Saved queries change from other tabs and the project; read them fresh on every opening.
    void s.loadSavedQueries(sessionId, database);
  }, [sessionId, database]);

  const { prefix, needle } = parsePrefix(query);
  const { hits, searching } = useRowHits(prefix, needle);
  const switchItems = useSwitchItems(prefix, needle);

  const items: Item[] = useMemo(() => {
    const rows = [...addressItems(prefix, needle), ...hitItems(hits)];
    const queries = savedItems(saved ?? [], session?.id ?? null, database, prefix, needle);
    return [...recentItems(recent, prefix, needle), ...rows, ...queries, ...switchItems, ...tableItems(tables ?? [], database, prefix, needle), ...commandItems(prefix, needle)].slice(0, MAX_ITEMS);
  }, [hits, needle, prefix, tables, recent, database, saved, session?.id, switchItems]);

  const hasEntities = !!info?.entities.length;
  return { items, searching, prefix, needle, rowSearch: searchesRows(prefix, needle), hasEntities };
}
