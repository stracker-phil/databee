import { useCallback, useEffect, useState } from "react";
import { ChevronLeft, ChevronRight, Copy, FileText, Filter, Loader2, RefreshCw } from "lucide-react";
import { api, toAppError, type AppError, type BrowsePage, type TableStructure } from "../api";
import { cellText } from "../lib/format";
import { selectWp, useStore, type TableTab as TableTabState } from "../state/store";
import { ErrorPanel } from "./QueryTab";
import { ResultsGrid } from "./ResultsGrid";

const MAX_PAGE = 500;

interface Browse {
  offset: number;
  orderBy: string | null;
  descending: boolean;
  filter: string;
}

function DataView({ tab }: { tab: TableTabState }) {
  const pageSize = useStore((s) => Math.min(s.settings?.rowLimit ?? 200, MAX_PAGE));
  const [browse, setBrowse] = useState<Browse>({ offset: 0, orderBy: null, descending: false, filter: tab.filter ?? "" });
  const [filterInput, setFilterInput] = useState(tab.filter ?? "");
  const entity = useStore((s) => (s.sessions.find((x) => x.id === tab.sessionId)?.database === tab.database ? selectWp(s)?.tableEntities[tab.table] : undefined));
  const openDocument = useStore((s) => s.openDocument);
  const [page, setPage] = useState<BrowsePage | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
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
      });
      setPage(result);
      setError(null);
    } catch (e) {
      setError(toAppError(e));
    } finally {
      setLoading(false);
    }
  }, [tab.sessionId, tab.database, tab.table, browse, pageSize]);

  useEffect(() => {
    void load();
  }, [load]);

  const sortBy = (column: string) =>
    setBrowse((b) => ({
      ...b,
      offset: 0,
      orderBy: b.orderBy === column && b.descending ? null : column,
      descending: b.orderBy === column && !b.descending,
    }));

  const rows = page?.result.rows.length ?? 0;
  return (
    <>
      <div className="table-head">
        <form
          className="filter"
          style={{ position: "relative" }}
          onSubmit={(e) => {
            e.preventDefault();
            setBrowse((b) => ({ ...b, offset: 0, filter: filterInput.trim() }));
          }}
        >
          <Filter size={12} style={{ position: "absolute", left: 8, top: 7, color: "var(--text-tertiary)" }} />
          <input
            className="input mono"
            style={{ paddingLeft: 24 }}
            aria-label="WHERE filter"
            placeholder="WHERE …   e.g. post_type = 'shop_order'   ↵ to apply"
            value={filterInput}
            onChange={(e) => setFilterInput(e.target.value)}
          />
        </form>
        <button className="icon-btn" aria-label="Reload" title="Reload" onClick={() => void load()}>
          {loading ? <Loader2 size={13} className="spin" /> : <RefreshCw size={13} />}
        </button>
        {entity && (
          <span className="hint" title="Right-click a row to open it as a document">
            <FileText size={11} style={{ verticalAlign: -1 }} /> {entity.entity} documents
          </span>
        )}
        <div className="spacer" />
        <span className="muted" style={{ fontVariantNumeric: "tabular-nums" }}>
          {rows === 0 ? "No rows" : `${browse.offset + 1}–${browse.offset + rows}`}
        </span>
        <button
          className="icon-btn"
          aria-label="Previous page"
          disabled={browse.offset === 0}
          onClick={() => setBrowse((b) => ({ ...b, offset: Math.max(0, b.offset - pageSize) }))}
        >
          <ChevronLeft size={14} />
        </button>
        <button
          className="icon-btn"
          aria-label="Next page"
          disabled={!page?.hasMore}
          onClick={() => setBrowse((b) => ({ ...b, offset: b.offset + pageSize }))}
        >
          <ChevronRight size={14} />
        </button>
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
          rowActions={
            entity
              ? (row) => {
                  const idIndex = page.result.columns.findIndex((c) => c.name.toLowerCase() === entity.idColumn.toLowerCase());
                  const id = idIndex >= 0 ? cellText(page.result.rows[row][idIndex]) : "";
                  return id ? [{ label: `Open ${entity.entity} #${id} as document`, action: () => openDocument(entity.entity, id) }] : [];
                }
              : undefined
          }
        />
      ) : (
        <div className="empty-state">Loading…</div>
      )}
    </>
  );
}

function StructureView({ tab }: { tab: TableTabState }) {
  const toast = useStore((s) => s.toast);
  const [structure, setStructure] = useState<TableStructure | null>(null);
  const [error, setError] = useState<AppError | null>(null);

  useEffect(() => {
    api.describeTable(tab.sessionId, tab.database, tab.table).then(
      (s) => {
        setStructure(s);
        setError(null);
      },
      (e) => setError(toAppError(e)),
    );
  }, [tab.sessionId, tab.database, tab.table]);

  if (error) return <ErrorPanel message={error.message} code={error.code} />;
  if (!structure) return <div className="empty-state">Loading…</div>;
  return (
    <div className="structure">
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
      <div>
        <h3 style={{ display: "flex", alignItems: "center", gap: 6 }}>
          DDL
          <button
            className="icon-btn"
            aria-label="Copy DDL"
            onClick={() => void navigator.clipboard.writeText(structure.ddl).then(() => toast("success", "Copied DDL"))}
          >
            <Copy size={12} />
          </button>
        </h3>
        <pre className="ddl">{structure.ddl}</pre>
      </div>
    </div>
  );
}

export function TableTab({ tab }: { tab: TableTabState }) {
  const updateTab = useStore((s) => s.updateTab);
  return (
    <>
      <div className="table-head" style={{ borderBottom: "none", paddingBottom: 0 }}>
        <strong>{tab.table}</strong>
        <span className="muted">{tab.database}</span>
        <div className="spacer" />
        <div className="segmented">
          <button aria-pressed={tab.view === "data"} onClick={() => updateTab(tab.id, { view: "data" })}>Data</button>
          <button aria-pressed={tab.view === "structure"} onClick={() => updateTab(tab.id, { view: "structure" })}>Structure</button>
        </div>
      </div>
      {tab.view === "data" ? <DataView tab={tab} /> : <StructureView tab={tab} />}
    </>
  );
}
