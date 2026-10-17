import { useEffect, useRef, useState } from "react";
import { AlertTriangle, ChevronLeft, ChevronRight, Columns3, Copy, Filter, Hash, Loader2, Plus, RefreshCw, X } from "lucide-react";
import { api, type AppliedFilter, type BrowsePage, type TableStructure } from "../api";
import { EVENTS, useBindingLabel } from "../commands";
import { address } from "../lib/address";
import { copyText } from "../lib/clipboard";
import { DEFAULT_ROW_LIMIT, singleKey } from "../lib/cells";
import { useApiResult } from "../lib/useApiResult";
import { valueAt } from "../lib/values";
import { selectStructure, useStore, type TableTab as TableTabState } from "../state/store";
import type { MenuEntry } from "./ContextMenu";
import { ErrorPanel } from "./ErrorPanel";
import { InsetField } from "./Field";
import { ResultsGrid } from "./ResultsGrid";
import { ValueCounts, type ValueCountsTarget } from "./ValueCounts";

/** Paging and sort; the filters live on the tab, which back and forward change. */
interface Browse {
  offset: number;
  orderBy: string | null;
  descending: boolean;
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
  const pk = structure?.primaryKey ?? [];
  return pk.length === 1 ? pk[0] : null;
}

/** Pages through the table once `ready`; a reply for an older page, sort or filter is dropped. */
function useBrowse(tab: TableTabState, primaryKey: string | null, ready: boolean) {
  const pageSize = useStore((s) => s.settings?.rowLimit ?? DEFAULT_ROW_LIMIT);
  const dataVersion = useStore((s) => s.dataVersion);
  const [browse, setBrowse] = useState<Browse>({ offset: 0, orderBy: null, descending: false });
  // Other rows (a new filter, back, forward) start at their first page and keep the sort.
  useEffect(() => setBrowse((b) => (b.offset === 0 ? b : { ...b, offset: 0 })), [tab.filter, tab.where]);
  const request = {
    database: tab.database,
    table: tab.table,
    offset: browse.offset,
    limit: pageSize,
    orderBy: browse.orderBy,
    descending: browse.descending,
    filter: tab.filter || null,
    primaryKey,
    where: tab.where,
  };
  const key = ready ? JSON.stringify([tab.sessionId, request, dataVersion]) : null;
  const result = useApiResult(key, () => api.browseTable(tab.sessionId, request), { keepData: true });
  return { browse, setBrowse, page: result.data, error: result.error, loading: key !== null && result.state === "loading", reload: result.reload, pageSize };
}

function FilterBar({ tabId, initial, primaryKey, onApply }: { tabId: string; initial: string; primaryKey: string | null; onApply: (filter: string) => void }) {
  const [value, setValue] = useState(initial);
  // Back and forward replace the applied filter; the field follows without losing focus.
  useEffect(() => setValue(initial), [initial]);
  const inputRef = useRef<HTMLInputElement>(null);
  const isActive = useStore((s) => s.activeTabId === tabId);
  const keys = useBindingLabel("table.focusFilter");

  useEffect(() => {
    if (!isActive) return;
    const focus = () => (inputRef.current?.focus(), inputRef.current?.select());
    window.addEventListener(EVENTS.focusWhere, focus);
    return () => window.removeEventListener(EVENTS.focusWhere, focus);
  }, [isActive]);

  return (
    <form
      className="filter"
      onSubmit={(e) => {
        e.preventDefault();
        onApply(value.trim());
      }}
    >
      <InsetField icon={<Filter size="0.846rem" />} keys={keys}>
        <input
          ref={inputRef}
          className="mono"
          aria-label="WHERE filter"
          data-stage="table:filter"
          placeholder={primaryKey ? "WHERE … or IDs: 3229, 3220-3229   ↵ to apply" : "WHERE …   e.g. post_type = 'shop_order'   ↵ to apply"}
          value={value}
          onChange={(e) => setValue(e.target.value)}
        />
      </InsetField>
    </form>
  );
}

/** The structured column filter as Rust ran it; read-only, it can only be cleared. */
function ColumnsFilterChip({ filter, onClear }: { filter: AppliedFilter; onClear: () => void }) {
  return (
    <span className="filter-chip" title={filter.sql}>
      <Columns3 size="0.846rem" /> <code>{filter.label}</code>
      <button className="icon-btn" aria-label="Clear column filter" onClick={onClear}>
        <X size="0.846rem" />
      </button>
    </span>
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

/** Row menu entries and cell edits, which address the row by the single-column key Rust sent with the page. */
function useRowEditing(tab: TableTabState, page: BrowsePage | null, primaryKey: string | null) {
  const slug = useStore((s) => s.sessions.find((x) => x.id === tab.sessionId)?.slug ?? "");
  const queueChange = useStore((s) => s.queueChange);
  const pinTab = useStore((s) => s.pinTab);
  const rowOf = (row: number) => (primaryKey && page ? singleKey(page.keys[row]) : null);

  const rowActions = (row: number): MenuEntry[] => {
    const key = rowOf(row);
    if (!key) return [];
    const base = { database: tab.database, table: tab.table, ...key };
    return [
      { label: "Open as object", action: () => useStore.getState().openObject(address(slug, tab.database, { kind: "row", table: tab.table, key: key.key })) },
      { label: "Duplicate row", action: () => void queueChange({ type: "duplicateRow", ...base }) },
      { label: "Delete row", action: () => void queueChange({ type: "deleteRow", ...base }) },
    ];
  };

  const editCell = (row: number, col: number, value: string | null) => {
    const key = rowOf(row);
    if (!key || !page) return;
    pinTab(tab.id);
    void queueChange({ type: "setCell", database: tab.database, table: tab.table, ...key, column: page.result.columns[col].name, value });
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
  const setTableFilter = useStore((s) => s.setTableFilter);
  const showRows = useStore((s) => s.showRows);
  const { rowActions, onEditCell } = useRowEditing(tab, page, primaryKey);
  const [showFilterSql, setShowFilterSql] = useState(false);
  const identity = useStore((s) => s.sessions.find((x) => x.id === tab.sessionId)?.identity ?? null);
  const [savedWidths, saveWidths] = useColumnWidths(identity, tab);
  const [counts, setCounts] = useState<ValueCountsTarget | null>(null);
  const filterRows = useStore((s) => s.filterRows);

  const columnMenu = (col: number, at: { x: number; y: number }): MenuEntry[] => {
    const column = page?.result.columns[col]?.name;
    if (!column) return [];
    const target = { sessionId: tab.sessionId, database: tab.database, table: tab.table, column, equals: tab.where, ...at };
    return [{ label: "Value counts", action: () => setCounts(target) }];
  };

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
        <FilterBar tabId={tab.id} initial={tab.filter ?? ""} primaryKey={primaryKey} onApply={(filter) => setTableFilter(tab.id, filter)} />
        {page?.columnsFilter && tab.where.length > 0 && (
          <ColumnsFilterChip filter={page.columnsFilter} onClear={() => showRows(tab.id, { database: tab.database, table: tab.table, where: [] })} />
        )}
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
        <button data-stage="table:reload" className="icon-btn" aria-label="Reload" title="Reload" onClick={reload}>
          {loading ? <Loader2 size="1rem" className="spin" /> : <RefreshCw size="1rem" />}
        </button>
        {structureError && (
          <span className="hint" role="alert" title={structureError.message}>
            <AlertTriangle size="0.846rem" style={{ verticalAlign: -1 }} /> Editing unavailable: the table structure did not load ({structureError.message})
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
          source={{ sessionId: tab.sessionId, resultId: page.result.id }}
          valueAt={(row, col) => valueAt(page, row, col)}
          sort={{ column: browse.orderBy, descending: browse.descending }}
          onSort={sortBy}
          columnWidths={savedWidths}
          onColumnWidthsChange={saveWidths}
          rowActions={rowActions}
          onEditCell={onEditCell}
          columnMenu={columnMenu}
        />
      ) : (
        <div className="empty-state">Loading…</div>
      )}
      {counts && <ValueCounts target={counts} onPick={(value) => filterRows(tab.id, { column: counts.column, value })} onClose={() => setCounts(null)} />}
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
  return (
    <div>
      <h3 style={{ display: "flex", alignItems: "center", gap: 6 }}>
        DDL
        <button
          className="icon-btn"
          aria-label="Copy DDL"
          onClick={() => void copyText(ddl, "DDL")}
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
