import { useEffect, useMemo } from "react";
import type { PaletteHit, TableInfo } from "../../api";
import { allCommands, bindingFor, bindingLabel, type Command } from "../../commands";
import { AddressError, formatAddress, parseAddress } from "../../lib/address";
import { projectKey } from "../../state/slices/project";
import { selectActiveSession, selectActiveTab, useStore, type RecentNav } from "../../state/store";
import { objectActionCommands } from "../object/actions";
import { parsePrefix, score, searchesRows, type Prefix } from "./parse";
import { useRowHits } from "./useRowHits";

export interface Item {
  key: string;
  /** Recent, a hit's section (`objects with id 4711`), Address, Tables or Commands. */
  section: string;
  title: string;
  subtitle?: string;
  badge?: string;
  binding?: string;
  /** A row hit, rendered with its values. */
  hit?: PaletteHit;
  go: () => void;
  /** ⌘↵: tables and addresses open in a kept tab, row hits in a new tab. */
  goAlt?: () => void;
}

const MAX_ITEMS = 80;
const MAX_TABLES = 40;

const recentTitle = (r: RecentNav) => (r.kind === "entity" ? r.title : `${r.database}.${r.table}`);

const close = () => useStore.getState().setPaletteOpen(false);

/** Enter: the next column of the active object tab, else an object tab; `newTab` always opens a tab. */
export function openHit(hit: PaletteHit, newTab: boolean) {
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

function recentItems(recent: RecentNav[], prefix: Prefix, needle: string): Item[] {
  if (needle || prefix !== null) return [];
  const { openEntity, openTableTab, setMode } = useStore.getState();
  return recent.map((r) => ({
    key: `recent:${recentTitle(r)}`,
    section: "Recent",
    title: recentTitle(r),
    badge: r.kind === "entity" ? r.entity : "table",
    go: () => {
      close();
      if (r.kind === "entity") void openEntity(r.entity, r.id);
      else (setMode("db"), openTableTab(r.database, r.table));
    },
  }));
}

function tableItems(tables: TableInfo[], database: string | null, prefix: Prefix, needle: string): Item[] {
  if (prefix !== null && prefix !== "t:") return [];
  const { openTableTab, setMode } = useStore.getState();
  return tables
    .map((t) => ({ t, s: score(t.name, needle) }))
    .filter(({ s }) => s > 0 && (needle || prefix === "t:"))
    .sort((a, b) => b.s - a.s)
    .slice(0, MAX_TABLES)
    .map(({ t }): Item => {
      const open = (preview: boolean) => {
        close();
        setMode("db");
        if (database) openTableTab(database, t.name, undefined, undefined, { preview });
      };
      return { key: `table:${t.name}`, section: "Tables", title: t.name, badge: t.isView ? "view" : "table", go: () => open(true), goAlt: () => open(false) };
    });
}

/** The palette's results for `query` in the order Recent, row hits, Tables, Commands. */
export function usePaletteItems(query: string) {
  const session = useStore(selectActiveSession);
  const database = session?.database ?? null;
  const tables = useStore((s) => (session && database ? s.schema[session.id]?.tables[database] : undefined));
  const info = useStore((s) => (session && database ? s.rulesInfo[projectKey(session.id, database)] : undefined));
  const recent = useStore((s) => s.recentNav);

  // Once per opening: the palette is remounted each time it opens.
  useEffect(() => {
    const { loadTables, loadRulesInfo } = useStore.getState();
    if (!session || !database) return;
    if (!tables) void loadTables(session.id, database);
    if (!info) void loadRulesInfo(session.id, database);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const { prefix, needle } = parsePrefix(query);
  const { hits, searching } = useRowHits(prefix, needle);

  const items: Item[] = useMemo(() => {
    const rows = [...addressItems(prefix, needle), ...hitItems(hits)];
    return [...recentItems(recent, prefix, needle), ...rows, ...tableItems(tables ?? [], database, prefix, needle), ...commandItems(prefix, needle)].slice(0, MAX_ITEMS);
  }, [hits, needle, prefix, tables, recent, database]);

  const hasEntities = !!info?.entities.length;
  return { items, searching, prefix, needle, rowSearch: searchesRows(prefix, needle), hasEntities };
}
