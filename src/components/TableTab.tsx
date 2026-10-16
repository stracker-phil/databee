import { useCallback, useEffect, useRef, useState } from "react";
import { AlertTriangle, ChevronLeft, ChevronRight, Copy, FileText, Filter, Hash, Loader2, Plus, RefreshCw } from "lucide-react";
import { api, toAppError, type AppError, type BrowsePage, type ColumnMeta, type TableStructure } from "../api";
import { EVENTS } from "../commands";
import { cellText } from "../lib/format";
import { selectStructure, selectWp, useStore, type TableTab as TableTabState } from "../state/store";
import type { MenuEntry } from "./ContextMenu";
import { ErrorPanel } from "./QueryTab";
import { ResultsGrid } from "./ResultsGrid";

/** Rust's default `rowLimit`, for the moment before settings load; Rust clamps the page size anyway. */
const DEFAULT_ROW_LIMIT = 1000;

interface Browse {
  offset: number;
  orderBy: string | null;
  descending: boolean;
  filter: string;
}

/** The table's structure from the store cache: one request per table and `dataVersion`, shared by all tabs. */
function useTableStructure(sessionId: string, database: string, table: string) {
  const entry = useStore((s) => selectStructure(s, sessionId, database, table));
  const dataVersion = useStore((s) => s.dataVersion);
  // `dataVersion` is only a refetch trigger: the effect reads nothing from it.
  useEffect(() => {
    void useStore.getState().loadStructure(sessionId, database, table);
  }, [sessionId, database, table, dataVersion]);
  return { structure: entry?.structure ?? null, error: entry?.error ?? null };
}

function singleColumnPrimaryKey(structure: TableStructure | null): string | null {
  const pk = structure?.indexes.find((i) => i.name === "PRIMARY");
  return pk?.columns.length === 1 ? pk.columns[0] : null;
}

/** MySQL column names are case-insensitive, so the result header may differ in case from the schema. */
function columnIndex(columns: ColumnMeta[], name: string): number {
  return columns.findIndex((c) => c.name.toLowerCase() === name.toLowerCase());
}

/** Pages through the table once `ready`; a reply that is not the latest request is dropped. */
function useBrowse(tab: TableTabState, primaryKey: string | null, ready: boolean) {
  const pageSize = useStore((s) => s.settings?.rowLimit ?? DEFAULT_ROW_LIMIT);
  const dataVersion = useStore((s) => s.dataVersion);
  const [browse, setBrowse] = useState<Browse>({ offset: 0, orderBy: null, descending: false, filter: tab.filter ?? "" });
  const [page, setPage] = useState<BrowsePage | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [loading, setLoading] = useState(false);
  const latest = useRef(0);

  const load = useCallback(async () => {
    const token = ++latest.current;
    setLoading(true);
    try {
      const result = await api.browseTable(tab.sessionId, {
        database: tab.database,
        table: tab.table,
        offset: browse.offset,
        limit: pageSize,
        orderBy: browse.orderBy,
        descending: browse.descending,
        filter: browse.filter || null,
        primaryKey,
      });
      if (token !== latest.current) return;
      setPage(result);
      setError(null);
    } catch (e) {
      if (token === latest.current) setError(toAppError(e));
    } finally {
      if (token === latest.current) setLoading(false);
    }
    // `dataVersion` is only a refetch trigger: a new value recreates `load`, which the effect below reruns.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab.sessionId, tab.database, tab.table, browse, pageSize, dataVersion, primaryKey]);

  useEffect(() => {
    if (ready) void load();
  }, [load, ready]);

  return { browse, setBrowse, page, error, loading, reload: load, pageSize };
}

function FilterBar({ tabId, initial, primaryKey, onApply }: { tabId: string; initial: string; primaryKey: string | null; onApply: (filter: string) => void }) {
  const [value, setValue] = useState(initial);
  const inputRef = useRef<HTMLInputElement>(null);
  const isActive = useStore((s) => s.activeTabId === tabId);

  useEffect(() => {
    if (!isActive) return;
    const focus = () => (inputRef.current?.focus(), inputRef.current?.select());
    window.addEventListener(EVENTS.focusWhere, focus);
    return () => window.removeEventListener(EVENTS.focusWhere, focus);
  }, [isActive]);

  return (
    <form
      className="filter"
      style={{ position: "relative" }}
      onSubmit={(e) => {
        e.preventDefault();
        onApply(value.trim());
      }}
    >
      <Filter size="0.923rem" style={{ position: "absolute", left: "0.615rem", top: "0.538rem", color: "var(--text-tertiary)" }} />
      <input
        ref={inputRef}
        className="input mono"
        style={{ paddingLeft: 24 }}
        aria-label="WHERE filter"
        data-stage="table:filter"
        placeholder={primaryKey ? `WHERE … or IDs: 3229, 3220-3229   ⌘F · ↵ to apply` : "WHERE …   e.g. post_type = 'shop_order'   ⌘F · ↵ to apply"}
        value={value}
        onChange={(e) => setValue(e.target.value)}
      />
    </form>
  );
}

interface PagerProps {
  offset: number;
  rows: number;
  /** The page size Rust served after its clamp, so offsets stay aligned with real pages. */
  limit: number;
  hasMore: boolean;
  onOffset: (offset: number) => void;
}

function Pager({ offset, rows, limit, hasMore, onOffset }: PagerProps) {
  return (
    <>
      <span className="muted" style={{ fontVariantNumeric: "tabular-nums" }}>
        {rows === 0 ? "No rows" : `${offset + 1}–${offset + rows}`}
      </span>
      <button className="icon-btn" aria-label="Previous page" data-stage="table:previous-page" disabled={offset === 0} onClick={() => onOffset(Math.max(0, offset - limit))}>
        <ChevronLeft size="1.077rem" />
      </button>
      <button className="icon-btn" aria-label="Next page" data-stage="table:next-page" disabled={!hasMore} onClick={() => onOffset(offset + limit)}>
        <ChevronRight size="1.077rem" />
      </button>
    </>
  );
}

/**
 * Row menu entries and cell edits. Duplicate, Delete and cell edits address the row by its single-column
 * primary key; "Open … in WordPress view" needs the WordPress entity's id column instead.
 */
function useRowEditing(tab: TableTabState, page: BrowsePage | null, primaryKey: string | null, entity: { entity: string; idColumn: string } | undefined) {
  const openEntity = useStore((s) => s.openEntity);
  const queueChange = useStore((s) => s.queueChange);
  const pinTab = useStore((s) => s.pinTab);
  const columns = page?.result.columns ?? [];
  const rows = page?.result.rows ?? [];
  const keyIndex = primaryKey ? columnIndex(columns, primaryKey) : -1;
  const keyOf = (row: number) => (keyIndex >= 0 ? cellText(rows[row][keyIndex]) : null);

  const rowActions = (row: number): MenuEntry[] => {
    const items: MenuEntry[] = [];
    const idIndex = entity ? columnIndex(columns, entity.idColumn) : -1;
    const id = idIndex >= 0 ? cellText(rows[row][idIndex]) : "";
    if (entity && id) items.push({ label: `Open ${entity.entity} #${id} in WordPress view`, action: () => openEntity(entity.entity, id) });
    const key = keyOf(row);
    if (primaryKey && key != null) {
      const base = { database: tab.database, table: tab.table, keyColumn: primaryKey, key };
      items.push({ label: "Duplicate row", action: () => void queueChange({ type: "duplicateRow", ...base }) });
      items.push({ label: "Delete row", action: () => void queueChange({ type: "deleteRow", ...base }) });
    }
    return items;
  };

  const editCell = (row: number, col: number, value: string | null) => {
    const key = keyOf(row);
    if (!primaryKey || key == null) return;
    pinTab(tab.id);
    void queueChange({ type: "setCell", database: tab.database, table: tab.table, keyColumn: primaryKey, key, column: columns[col].name, value });
  };

  return { rowActions, onEditCell: primaryKey ? editCell : undefined };
}

/** Column widths saved per connection identity and table; without an identity nothing is saved. */
function useColumnWidths(identity: string | null, tab: TableTabState) {
  const table = `${tab.database}.${tab.table}`;
  const savedWidths = useStore((s) => (identity ? s.uiState[identity]?.tables[table]?.widths : undefined));
  const setTableWidths = useStore((s) => s.setTableWidths);
  const saveWidths = (widths: Record<string, number>) => {
    if (identity) setTableWidths(identity, table, widths);
  };
  return [savedWidths, saveWidths] as const;
}

function DataView({ tab }: { tab: TableTabState }) {
  const { structure, error: structureError } = useTableStructure(tab.sessionId, tab.database, tab.table);
  const primaryKey = singleColumnPrimaryKey(structure);
  // Browsing waits for the structure so an ID filter is resolved against the primary key from the first request.
  const { browse, setBrowse, page, error, loading, reload, pageSize } = useBrowse(tab, primaryKey, structure !== null || structureError !== null);
  const entity = useStore((s) => (s.sessions.find((x) => x.id === tab.sessionId)?.database === tab.database ? selectWp(s)?.tableEntities[tab.table] : undefined));
  const { rowActions, onEditCell } = useRowEditing(tab, page, primaryKey, entity);
  const [showFilterSql, setShowFilterSql] = useState(false);
  const identity = useStore((s) => s.sessions.find((x) => x.id === tab.sessionId)?.identity ?? null);
  const [savedWidths, saveWidths] = useColumnWidths(identity, tab);

  const sortBy = (column: string) =>
    setBrowse((b) => ({
      ...b,
      offset: 0,
      orderBy: b.orderBy === column && b.descending ? null : column,
      descending: b.orderBy === column && !b.descending,
    }));

  return (
    <>
      <div className="table-head">
        <FilterBar tabId={tab.id} initial={tab.filter ?? ""} primaryKey={primaryKey} onApply={(filter) => setBrowse((b) => ({ ...b, offset: 0, filter }))} />
        {page?.filter?.kind === "ids" && (
          <button className="filter-chip" title="Interpreted as a primary-key lookup; click to see the SQL" onClick={() => setShowFilterSql((v) => !v)}>
            <Hash size="0.846rem" /> {showFilterSql ? <code>{page.filter.sql}</code> : page.filter.label}
          </button>
        )}
        <button
          className="icon-btn"
          aria-label="Insert row"
          data-stage="table:insert"
          title={primaryKey ? "Insert row" : "Editing needs a single-column primary key"}
          disabled={!structure}
          onClick={() => structure && useStore.getState().openDialog({ type: "insertRow", database: tab.database, table: tab.table, columns: structure.columns })}
        >
          <Plus size="1rem" />
        </button>
        <button data-stage="table:reload" className="icon-btn" aria-label="Reload" title="Reload" onClick={() => void reload()}>
          {loading ? <Loader2 size="1rem" className="spin" /> : <RefreshCw size="1rem" />}
        </button>
        {structureError && (
          <span className="hint" role="alert" title={structureError.message}>
            <AlertTriangle size="0.846rem" style={{ verticalAlign: -1 }} /> Editing unavailable: the table structure did not load ({structureError.message})
          </span>
        )}
        {entity && (
          <span className="hint" title="Right-click a row to open it as a document">
            <FileText size="0.846rem" style={{ verticalAlign: -1 }} /> {entity.entity} documents
          </span>
        )}
        <div className="spacer" />
        <Pager
          offset={browse.offset}
          rows={page?.result.rows.length ?? 0}
          limit={page?.limit ?? pageSize}
          hasMore={!!page?.hasMore}
          onOffset={(offset) => setBrowse((b) => ({ ...b, offset }))}
        />
      </div>
      {error ? (
        <ErrorPanel message={error.message} code={error.code} />
      ) : page ? (
        <ResultsGrid
          columns={page.result.columns}
          rows={page.result.rows}
          rowOffset={browse.offset}
          sort={{ column: browse.orderBy, descending: browse.descending }}
          onSort={sortBy}
          columnWidths={savedWidths}
          onColumnWidthsChange={saveWidths}
          rowActions={rowActions}
          onEditCell={onEditCell}
        />
      ) : (
        <div className="empty-state">Loading…</div>
      )}
    </>
  );
}

function StructureView({ tab }: { tab: TableTabState }) {
  const { structure, error } = useTableStructure(tab.sessionId, tab.database, tab.table);

  if (error) return <ErrorPanel message={error.message} code={error.code} />;
  if (!structure) return <div className="empty-state">Loading…</div>;
  return (
    <div className="structure" data-stage="table:structure-view" data-stage-label="Table structure">
      <div>
        <h3>Columns</h3>
        <table>
          <thead>
            <tr>
              <th>Name</th><th>Type</th><th>Null</th><th>Default</th><th>Key</th><th>Extra</th><th>Comment</th>
            </tr>
          </thead>
          <tbody>
            {structure.columns.map((c) => (
              <tr key={c.name}>
                <td style={{ fontWeight: 600 }}>{c.name}</td>
                <td className="mono">{c.columnType}</td>
                <td>{c.nullable ? "YES" : ""}</td>
                <td className="mono">{c.default ?? <span className="muted">—</span>}</td>
                <td>{c.key && <span className="badge">{c.key}</span>}</td>
                <td className="muted">{c.extra}</td>
                <td className="muted">{c.comment}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div>
        <h3>Indexes</h3>
        <table>
          <thead>
            <tr><th>Name</th><th>Columns</th><th>Unique</th></tr>
          </thead>
          <tbody>
            {structure.indexes.map((i) => (
              <tr key={i.name}>
                <td style={{ fontWeight: 600 }}>{i.name}</td>
                <td className="mono">{i.columns.join(", ")}</td>
                <td>{i.unique ? "YES" : ""}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {structure.ddl !== null && <Ddl ddl={structure.ddl} />}
    </div>
  );
}

function Ddl({ ddl }: { ddl: string }) {
  const toast = useStore((s) => s.toast);
  return (
    <div>
      <h3 style={{ display: "flex", alignItems: "center", gap: 6 }}>
        DDL
        <button
          className="icon-btn"
          aria-label="Copy DDL"
          onClick={() => void navigator.clipboard.writeText(ddl).then(() => toast("success", "Copied DDL"))}
        >
          <Copy size="0.923rem" />
        </button>
      </h3>
      <pre className="ddl">{ddl}</pre>
    </div>
  );
}

export function TableTab({ tab }: { tab: TableTabState }) {
  const updateTableTab = useStore((s) => s.updateTableTab);
  return (
    <>
      <div className="table-head" style={{ borderBottom: "none", paddingBottom: 0 }}>
        <strong>{tab.table}</strong>
        <span className="muted">{tab.database}</span>
        <div className="spacer" />
        <div className="segmented" data-stage="table:view" data-stage-label="Data / Structure">
          <button data-stage="table:data" aria-pressed={tab.view === "data"} onClick={() => updateTableTab(tab.id, { view: "data" })}>Data</button>
          <button data-stage="table:structure" aria-pressed={tab.view === "structure"} onClick={() => updateTableTab(tab.id, { view: "structure" })}>Structure</button>
        </div>
      </div>
      {tab.view === "data" ? <DataView tab={tab} /> : <StructureView tab={tab} />}
    </>
  );
}
