import { useEffect, useMemo, useRef } from "react";
import {
  api,
  type Address,
  type BrowsePage,
  type ColumnMeta,
  type Document,
  type Group,
  type GroupRow,
  type Provenance,
  type RowKey,
  type Value,
} from "../../api";
import { columnIndex, DEFAULT_ROW_LIMIT, singleKey } from "../../lib/cells";
import { fieldOf, groupAnchors, isEmpty, provenanceKey } from "../../lib/document";
import { useApiResult } from "../../lib/useApiResult";
import { valueAt } from "../../lib/values";
import { selectSessionBySlug, useStore } from "../../state/store";
import { ErrorPanel } from "../ErrorPanel";
import { ResultsGrid, type CellPos } from "../ResultsGrid";
import { GroupHeader } from "./GroupHeader";
import { useCollapsedGroups } from "./useCollapsedGroups";

const sameKey = (a: RowKey, b: RowKey) =>
  a.length === b.length && a.every((p, i) => p.column.toLowerCase() === b[i].column.toLowerCase() && p.value === b[i].value);

const useSessionId = (owner: Address) => useStore((s) => selectSessionBySlug(s, owner.connection)?.id ?? null);

/** The group's rows as a browse page; not browsed for a group whose rows no address names (patterns). */
function useGroupPage(sessionId: string | null, database: string, group: Group) {
  const limit = useStore((s) => s.settings?.rowLimit ?? DEFAULT_ROW_LIMIT);
  const dataVersion = useStore((s) => s.dataVersion);
  const target = group.rowsAddress?.target;
  const where = target?.kind === "table" ? target.filter : null;
  const request = where && sessionId ? { database, table: group.table, offset: 0, limit, where } : null;
  const key = request ? JSON.stringify([sessionId, request, dataVersion]) : null;
  const { data: page, error } = useApiResult(key, () => api.browseTable(sessionId!, request!), { keepData: true });
  return { browsed: !!where, page, error };
}

interface GridProps {
  sessionId: string;
  owner: Address;
  focus: Provenance | null;
}

function BrowsedGrid({ page, sessionId, owner, focus }: GridProps & { page: BrowsePage }) {
  const cell = useMemo((): CellPos | null => {
    if (focus?.table !== page.table) return null;
    const row = page.keys.findIndex((k) => sameKey(k, focus.row));
    const col = columnIndex(page.result.columns, focus.column);
    return row >= 0 && col >= 0 ? { row, col } : null;
  }, [focus, page]);
  const onEditCell = (row: number, col: number, value: string | null) => {
    const key = singleKey(page.keys[row]);
    if (!key) return;
    void useStore.getState().queueChange({ type: "setCell", database: owner.database, table: page.table, ...key, column: page.result.columns[col].name, value });
  };
  return (
    <div className="rows-grid">
      <ResultsGrid
        columns={page.result.columns}
        rows={page.result.rows}
        source={{ sessionId, resultId: page.result.id }}
        valueAt={(row, col) => valueAt(page, row, col)}
        onEditCell={onEditCell}
        owner={owner}
        focus={cell}
      />
    </div>
  );
}

const NULL_VALUE: Value = {
  raw: { t: "null" },
  kind: { kind: "null" },
  source: null,
  style: null,
  secondary: null,
};

const rowValues = (group: Group, row: GroupRow) => group.columns.map((column, i) => fieldOf(row, column, i) ?? NULL_VALUE);

function cellOf(values: Value[][], focus: Provenance | null): CellPos | null {
  if (!focus) return null;
  const key = provenanceKey(focus);
  for (const [row, cells] of values.entries()) {
    const col = cells.findIndex((v) => v.source && provenanceKey(v.source) === key);
    if (col >= 0) return { row, col };
  }
  return null;
}

/** A group no address names (patterns): its document rows as a grid, read-only. */
function DocumentGrid({ group, sessionId, owner, focus }: GridProps & { group: Group }) {
  const columns = useMemo((): ColumnMeta[] => group.columns.map((name) => ({ name, typeName: "", origin: null })), [group]);
  const values = useMemo(() => group.rows.map((row) => rowValues(group, row)), [group]);
  const rows = useMemo(() => values.map((row) => row.map((v) => v.raw)), [values]);
  const cell = useMemo(() => cellOf(values, focus), [values, focus]);
  return (
    <div className="rows-grid">
      <ResultsGrid columns={columns} rows={rows} source={{ sessionId, resultId: 0 }} valueAt={(row, col) => values[row][col]} owner={owner} focus={cell} />
    </div>
  );
}

function GroupGrid({ group, owner, focus }: { group: Group; owner: Address; focus: Provenance | null }) {
  const sessionId = useSessionId(owner);
  const { browsed, page, error } = useGroupPage(sessionId, owner.database, group);
  if (!sessionId) return null;
  if (!browsed) return <DocumentGrid group={group} sessionId={sessionId} owner={owner} focus={focus} />;
  if (error) return <ErrorPanel message={error.message} code={error.code} />;
  if (!page) return <div className="also">Loading…</div>;
  return <BrowsedGrid page={page} sessionId={sessionId} owner={owner} focus={focus} />;
}

function filterText(group: Group): string | null {
  const target = group.rowsAddress?.target;
  if (target?.kind !== "table" || target.filter.length === 0) return null;
  return target.filter.map((f) => (f.value === null ? `${f.column} IS NULL` : `${f.column} = ${f.value}`)).join(" AND ");
}

interface Props {
  document: Document;
  owner: Address;
  focus: Provenance | null;
  /** The group to scroll to, set by a capped group's `+n more`. */
  scrollTo: string | null;
}

export function RowsView({ document, owner, focus, scrollTo }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const anchors = groupAnchors(document.groups);
  const groups = document.groups.filter((g) => g.table && !isEmpty(g));
  const collapse = useCollapsedGroups(owner);
  useEffect(() => {
    if (!scrollTo) return;
    const sections = ref.current?.querySelectorAll<HTMLElement>("[data-group]") ?? [];
    [...sections].find((el) => el.dataset.group === scrollTo)?.scrollIntoView?.({ block: "start" });
  }, [scrollTo]);
  return (
    <div className="document rows-view" ref={ref}>
      {groups.map((group) => {
        const filter = filterText(group);
        const collapsed = collapse.isCollapsed(group.id);
        return (
          <section key={group.id} data-group={group.id} aria-label={`${group.table} rows`}>
            <GroupHeader group={group} database={owner.database} anchor={anchors.get(group.id)} collapsed={collapsed} onToggle={collapse.toggle} />
            {!collapsed && filter && <div className="rows-filter">WHERE {filter}</div>}
            {!collapsed && (group.error ? <ErrorPanel message={group.error} code={null} /> : <GroupGrid group={group} owner={owner} focus={focus} />)}
          </section>
        );
      })}
    </div>
  );
}
