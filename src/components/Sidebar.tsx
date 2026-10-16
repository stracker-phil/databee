import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import { ChevronDown, ChevronRight, Database, Eye, History, RefreshCw, Search, Table2, Trash2 } from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import { api, type HistoryEntry, type TableInfo } from "../api";
import { EVENTS } from "../commands";
import { copyText } from "../lib/clipboard";
import { compactCount, formatBytes, formatCount, relativeTime } from "../lib/format";
import { fail, selectActiveSession, selectActiveTab, selectVisibleDatabases, useStore } from "../state/store";
import { ContextMenu, type MenuEntry } from "./ContextMenu";

const dbKey = (name: string) => `db:${name}`;
const tableKey = (db: string, name: string) => `table:${db}.${name}`;

/** One focusable row of the tables tree: a database, or a table inside it. */
type TreeRow = { key: string; db: string; table?: undefined; open: boolean } | TableRow;

interface TableRow {
  key: string;
  db: string;
  table: TableInfo;
}

interface TreeActions {
  toggle: (db: string) => void;
  openTable: (row: TableRow) => void;
}

/** Roving tabIndex over the visible rows: one row is tabbable, arrows move focus and expand or collapse. */
function useTreeFocus(rows: TreeRow[], { toggle, openTable }: TreeActions) {
  const [focused, setFocused] = useState<string | null>(null);
  const rowRefs = useRef(new Map<string, HTMLDivElement>());
  const tabStop = rows.some((r) => r.key === focused) ? focused : rows[0]?.key;

  const focusRow = (key: string | undefined) => {
    if (!key) return;
    setFocused(key);
    rowRefs.current.get(key)?.focus();
  };

  const onRowKey = (e: KeyboardEvent, row: TreeRow) => {
    const i = rows.findIndex((r) => r.key === row.key);
    switch (e.key) {
      case "ArrowDown":
        focusRow(rows[i + 1]?.key);
        break;
      case "ArrowUp":
        focusRow(rows[i - 1]?.key);
        break;
      case "ArrowRight":
        if (row.table) return;
        if (!row.open) toggle(row.db);
        else if (rows[i + 1]?.table) focusRow(rows[i + 1].key);
        break;
      case "ArrowLeft":
        if (row.table) focusRow(dbKey(row.db));
        else if (row.open) toggle(row.db);
        break;
      case "Enter":
        if (row.table) openTable(row);
        else toggle(row.db);
        break;
      default:
        return;
    }
    e.preventDefault();
  };

  return (row: TreeRow) => ({
    ref: (el: HTMLDivElement | null) => {
      if (el) rowRefs.current.set(row.key, el);
      else rowRefs.current.delete(row.key);
    },
    role: "treeitem",
    tabIndex: row.key === tabStop ? 0 : -1,
    onFocus: () => setFocused(row.key),
    onKeyDown: (e: KeyboardEvent) => onRowKey(e, row),
  });
}

function TablesPanel() {
  const session = useStore(selectActiveSession);
  const schema = useStore((s) => (session ? s.schema[session.id] : undefined));
  const databases = useStore(useShallow(selectVisibleDatabases));
  const activeTab = useStore(selectActiveTab);
  const { loadTables, loadDatabases, openTableTab, openQueryTab, addFavorite } = useStore.getState();
  const [filter, setFilter] = useState("");
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuEntry[] } | null>(null);
  const filterRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const focus = () => filterRef.current?.focus();
    window.addEventListener(EVENTS.focusTableFilter, focus);
    return () => window.removeEventListener(EVENTS.focusTableFilter, focus);
  }, []);

  const current = session?.database ?? null;
  const isOpen = (db: string) => expanded[db] ?? db === current;
  const needle = filter.trim().toLowerCase();

  const toggle = (db: string) => {
    const open = !isOpen(db);
    setExpanded((e) => ({ ...e, [db]: open }));
    if (open && session && !schema?.tables[db]) void loadTables(session.id, db);
  };

  const openPreview = (db: string, table: string, preview = true) => openTableTab(db, table, "data", undefined, { preview });

  const openSelectQuery = async (db: string, table: string) => {
    if (!session) return;
    try {
      openQueryTab(await api.tableSelectSql(session.id, db, table));
    } catch (e) {
      fail(e, "Could not build the query");
    }
  };

  const tree = databases.map((d) => {
    const tableRows = (schema?.tables[d.name] ?? [])
      .filter((t) => !needle || t.name.toLowerCase().includes(needle))
      .map((t): TableRow => ({ key: tableKey(d.name, t.name), db: d.name, table: t }));
    const open = isOpen(d.name) || (!!needle && tableRows.length > 0);
    return { d, row: { key: dbKey(d.name), db: d.name, open }, tableRows };
  });
  const rows: TreeRow[] = tree.flatMap(({ row, tableRows }) => [row, ...(row.open ? tableRows : [])]);
  const rowProps = useTreeFocus(rows, { toggle, openTable: (row) => openPreview(row.db, row.table.name) });

  if (!session) return null;

  const tableMenu = (db: string, t: TableInfo, e: MouseEvent) => {
    e.preventDefault();
    setMenu({
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: "Open data", action: () => openTableTab(db, t.name, "data") },
        { label: "Show structure", action: () => openTableTab(db, t.name, "structure") },
        { label: "New query: SELECT …", action: () => void openSelectQuery(db, t.name) },
        "separator",
        { label: "Add to favorites", action: () => void addFavorite(t.name, { kind: "table", database: db, table: t.name }) },
        { label: "Copy name", action: () => void copyText(t.name, "name") },
      ],
    });
  };

  return (
    <>
      <div className="sidebar-head">
        <div style={{ position: "relative", flex: 1 }}>
          <Search size="0.923rem" style={{ position: "absolute", left: "0.615rem", top: "0.538rem", color: "var(--text-tertiary)" }} />
          <input
            ref={filterRef}
            className="input"
            style={{ paddingLeft: 24 }}
            placeholder="Filter tables  ⌘⇧F"
            aria-label="Filter tables"
            data-stage="sidebar:filter"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") setFilter("");
            }}
          />
        </div>
        <button data-stage="sidebar:refresh" className="icon-btn" aria-label="Refresh" title="Refresh" onClick={() => void loadDatabases(session.id)}>
          <RefreshCw size="1rem" />
        </button>
      </div>
      <div className="sidebar-body" role="tree" aria-label="Databases">
        {tree.map(({ d, row, tableRows }) => (
          <div key={d.name} role="none">
            <div
              {...rowProps(row)}
              aria-level={1}
              aria-expanded={row.open}
              className={`tree-row${d.isSystem ? " system" : ""}`}
              data-stage={`database:${d.name}`}
              data-stage-list="databases"
              data-stage-label={d.name}
              onClick={() => toggle(d.name)}
            >
              {row.open ? <ChevronDown size="0.923rem" /> : <ChevronRight size="0.923rem" />}
              <Database size="1rem" />
              <span className="label" style={{ fontWeight: d.name === current ? 600 : 400 }}>{d.name}</span>
              {schema?.tables[d.name] && <span className="meta">{schema.tables[d.name].length}</span>}
            </div>
            {row.open && (
              <div className="tree-children" role="group">
                {!schema?.tables[d.name] && <div className="sidebar-empty">Loading…</div>}
                {schema?.tables[d.name] && tableRows.length === 0 && (
                  <div className="sidebar-empty">{needle ? "No matching tables" : "No tables"}</div>
                )}
                {tableRows.map((tr) => {
                  const t = tr.table;
                  const active = activeTab?.kind === "table" && activeTab.database === d.name && activeTab.table === t.name;
                  return (
                    <div
                      key={t.name}
                      {...rowProps(tr)}
                      aria-level={2}
                      data-stage={`table:${d.name}.${t.name}`}
                      data-stage-list="tables"
                      aria-selected={active}
                      data-stage-label={t.name}
                      className={`tree-row${active ? " active" : ""}`}
                      title={[t.comment, t.rowsEstimate != null ? `~${formatCount(t.rowsEstimate)} rows` : "", formatBytes(t.sizeBytes)].filter(Boolean).join(" · ")}
                      onClick={(e) => openPreview(d.name, t.name, !e.metaKey)}
                      onDoubleClick={() => openTableTab(d.name, t.name, undefined, undefined, { preview: false })}
                      onContextMenu={(e) => tableMenu(d.name, t, e)}
                    >
                      {t.isView ? <Eye size="1rem" /> : <Table2 size="1rem" />}
                      <span className="label">{t.name}</span>
                      {t.rowsEstimate != null && !t.isView && <span className="meta">{compactCount(t.rowsEstimate)}</span>}
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        ))}
      </div>
      {menu && <ContextMenu {...menu} onClose={() => setMenu(null)} />}
    </>
  );
}

function HistoryPanel() {
  const session = useStore(selectActiveSession);
  const historyVersion = useStore((s) => s.historyVersion);
  const openQueryTab = useStore((s) => s.openQueryTab);
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const account = session?.account ?? null;

  useEffect(() => {
    if (!account) return;
    // Only the newest request writes: a slow reply for another server or an earlier run is dropped.
    let latest = true;
    api.listHistory(account).then(
      (list) => {
        if (latest) setEntries(list);
      },
      () => {
        if (latest) setEntries([]);
      },
    );
    return () => {
      latest = false;
    };
  }, [account, historyVersion]);

  if (!account) return null;

  const clear = async () => {
    try {
      await api.clearHistory(account);
      setEntries([]);
    } catch (e) {
      fail(e, "Could not clear history");
    }
  };

  return (
    <>
      <div className="sidebar-head">
        <span className="muted" style={{ flex: 1, paddingLeft: 4 }}>{entries.length} queries</span>
        <button className="icon-btn" aria-label="Clear history" title="Clear history" disabled={entries.length === 0} onClick={() => void clear()}>
          <Trash2 size="1rem" />
        </button>
      </div>
      <div className="sidebar-body" data-stage="sidebar:history-list" data-stage-label="Query history">
        {entries.length === 0 && <div className="sidebar-empty">Queries you run on this server appear here.</div>}
        {entries.map((e, i) => (
          <div
            // Entries carry no id, and executedAt + sql can repeat.
            key={i}
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
    <aside className="sidebar" data-stage="sidebar" data-stage-label="Sidebar">
      <div style={{ padding: "8px 8px 0" }}>
        <div className="segmented" style={{ width: "100%" }}>
          <button data-stage="sidebar:tables" style={{ flex: 1 }} aria-pressed={panel === "tables"} onClick={() => setPanel("tables")}>
            <Table2 size="0.923rem" style={{ verticalAlign: -2 }} /> Tables
          </button>
          <button data-stage="sidebar:history" style={{ flex: 1 }} aria-pressed={panel === "history"} onClick={() => setPanel("history")}>
            <History size="0.923rem" style={{ verticalAlign: -2 }} /> History
          </button>
        </div>
      </div>
      {panel === "tables" ? <TablesPanel /> : <HistoryPanel />}
    </aside>
  );
}
