import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown, ChevronRight, Database, Eye, History, RefreshCw, Search, Table2, Trash2 } from "lucide-react";
import { accountOf, api, type HistoryEntry, type TableInfo } from "../api";
import { formatBytes, formatCount, relativeTime } from "../lib/format";
import { selectActiveSession, selectActiveTab, useStore } from "../state/store";
import { ContextMenu, type MenuEntry } from "./ContextMenu";

export const FOCUS_FILTER_EVENT = "databee:focus-filter";

function TablesPanel() {
  const session = useStore(selectActiveSession);
  const schema = useStore((s) => (session ? s.schema[session.id] : undefined));
  const showSystem = useStore((s) => s.settings?.showSystemSchemas ?? false);
  const activeTab = useStore(selectActiveTab);
  const { loadTables, loadDatabases, openTableTab, openQueryTab, addFavorite } = useStore.getState();
  const [filter, setFilter] = useState("");
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuEntry[] } | null>(null);
  const filterRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const focus = () => filterRef.current?.focus();
    window.addEventListener(FOCUS_FILTER_EVENT, focus);
    return () => window.removeEventListener(FOCUS_FILTER_EVENT, focus);
  }, []);

  const current = session?.database ?? null;
  const isOpen = (db: string) => expanded[db] ?? db === current;
  const needle = filter.trim().toLowerCase();

  const databases = useMemo(
    () => (schema?.databases ?? []).filter((d) => showSystem || !d.isSystem || d.name === current),
    [schema, showSystem, current],
  );

  if (!session) return null;

  const toggle = (db: string) => {
    const open = !isOpen(db);
    setExpanded((e) => ({ ...e, [db]: open }));
    if (open && !schema?.tables[db]) void loadTables(session.id, db);
  };

  const tableMenu = (db: string, t: TableInfo, e: React.MouseEvent) => {
    e.preventDefault();
    setMenu({
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: "Open data", action: () => openTableTab(db, t.name, "data") },
        { label: "Show structure", action: () => openTableTab(db, t.name, "structure") },
        { label: "New query: SELECT …", action: () => openQueryTab(`SELECT *\nFROM \`${db}\`.\`${t.name}\`\nLIMIT 100;`) },
        "separator",
        { label: "Add to favorites", action: () => void addFavorite(t.name, { kind: "table", database: db, table: t.name }) },
        { label: "Copy name", action: () => void navigator.clipboard.writeText(t.name) },
      ],
    });
  };

  const visibleTables = (db: string) =>
    (schema?.tables[db] ?? []).filter((t) => !needle || t.name.toLowerCase().includes(needle));

  return (
    <>
      <div className="sidebar-head">
        <div style={{ position: "relative", flex: 1 }}>
          <Search size={12} style={{ position: "absolute", left: 8, top: 7, color: "var(--text-tertiary)" }} />
          <input
            ref={filterRef}
            className="input"
            style={{ paddingLeft: 24 }}
            placeholder="Filter tables  ⌘K"
            aria-label="Filter tables"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            onKeyDown={(e) => e.key === "Escape" && setFilter("")}
          />
        </div>
        <button className="icon-btn" aria-label="Refresh" title="Refresh" onClick={() => void loadDatabases(session.id)}>
          <RefreshCw size={13} />
        </button>
      </div>
      <div className="sidebar-body" role="tree" aria-label="Databases">
        {databases.map((d) => {
          const open = isOpen(d.name) || (!!needle && visibleTables(d.name).length > 0);
          const tables = visibleTables(d.name);
          return (
            <div key={d.name} role="treeitem" aria-expanded={open}>
              <div className={`tree-row${d.isSystem ? " system" : ""}`} onClick={() => toggle(d.name)}>
                {open ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
                <Database size={13} />
                <span className="label" style={{ fontWeight: d.name === current ? 600 : 400 }}>{d.name}</span>
                {schema?.tables[d.name] && <span className="meta">{schema.tables[d.name].length}</span>}
              </div>
              {open && (
                <div className="tree-children" role="group">
                  {!schema?.tables[d.name] && <div className="sidebar-empty">Loading…</div>}
                  {schema?.tables[d.name] && tables.length === 0 && (
                    <div className="sidebar-empty">{needle ? "No matching tables" : "No tables"}</div>
                  )}
                  {tables.map((t) => {
                    const active = activeTab?.kind === "table" && activeTab.database === d.name && activeTab.table === t.name;
                    return (
                      <div
                        key={t.name}
                        role="treeitem"
                        className={`tree-row${active ? " active" : ""}`}
                        title={[t.comment, t.rowsEstimate != null ? `~${formatCount(t.rowsEstimate)} rows` : "", formatBytes(t.sizeBytes)].filter(Boolean).join(" · ")}
                        onClick={() => openTableTab(d.name, t.name)}
                        onContextMenu={(e) => tableMenu(d.name, t, e)}
                      >
                        {t.isView ? <Eye size={13} /> : <Table2 size={13} />}
                        <span className="label">{t.name}</span>
                        {t.rowsEstimate != null && !t.isView && <span className="meta">{compact(t.rowsEstimate)}</span>}
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          );
        })}
      </div>
      {menu && <ContextMenu {...menu} onClose={() => setMenu(null)} />}
    </>
  );
}

function compact(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${(n / 1000).toFixed(n < 10_000 ? 1 : 0)}k`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}

function HistoryPanel() {
  const session = useStore(selectActiveSession);
  const runningCount = useStore((s) => s.tabs.filter((t) => t.kind === "query" && t.running).length);
  const openQueryTab = useStore((s) => s.openQueryTab);
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const account = session ? accountOf(session.spec) : null;

  useEffect(() => {
    if (!account) return;
    api.listHistory(account).then(setEntries, () => setEntries([]));
  }, [account, runningCount]);

  if (!account) return null;
  return (
    <>
      <div className="sidebar-head">
        <span className="muted" style={{ flex: 1, paddingLeft: 4 }}>{entries.length} queries</span>
        <button
          className="icon-btn"
          aria-label="Clear history"
          title="Clear history"
          disabled={entries.length === 0}
          onClick={async () => {
            await api.clearHistory(account).catch(() => {});
            setEntries([]);
          }}
        >
          <Trash2 size={13} />
        </button>
      </div>
      <div className="sidebar-body">
        {entries.length === 0 && <div className="sidebar-empty">Queries you run on this server appear here.</div>}
        {entries.map((e) => (
          <div
            key={e.executedAt + e.sql}
            className={`history-item${e.error ? " failed" : ""}`}
            title="Double-click to open in a new tab"
            onDoubleClick={() => openQueryTab(e.sql)}
          >
            <div className="sql">{e.sql}</div>
            <div className="when">
              <span>{relativeTime(e.executedAt)}</span>
              {e.database && <span>{e.database}</span>}
              <span>{e.error ? "failed" : `${e.elapsedMs} ms`}</span>
            </div>
          </div>
        ))}
      </div>
    </>
  );
}

export function Sidebar() {
  const panel = useStore((s) => s.sidebarPanel);
  const setPanel = useStore((s) => s.setSidebarPanel);
  return (
    <aside className="sidebar">
      <div style={{ padding: "8px 8px 0" }}>
        <div className="segmented" style={{ width: "100%" }}>
          <button style={{ flex: 1 }} aria-pressed={panel === "tables"} onClick={() => setPanel("tables")}>
            <Table2 size={12} style={{ verticalAlign: -2 }} /> Tables
          </button>
          <button style={{ flex: 1 }} aria-pressed={panel === "history"} onClick={() => setPanel("history")}>
            <History size={12} style={{ verticalAlign: -2 }} /> History
          </button>
        </div>
      </div>
      {panel === "tables" ? <TablesPanel /> : <HistoryPanel />}
    </aside>
  );
}
