/** The readout tab (`databee://<slug>/<db>/`): status header, anomalies, new, pinned and recent; every line opens its address. */
import { useEffect, type MouseEvent, type ReactNode } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import type { Address, AnomalyLine, NewObjects, Part, PinnedLine, ReadoutRow, RecentLine, Status, Value as StoredValue } from "../../api";
import { formatAddress } from "../../lib/address";
import { formatCount, relativeTime } from "../../lib/format";
import { readoutKey } from "../../state/slices/readout";
import { useStore, type ReadoutTab } from "../../state/store";
import { ErrorPanel } from "../ErrorPanel";
import { useCollapsedGroups } from "../object/useCollapsedGroups";
import { useOpenValue } from "../value/useOpenValue";
import { Value, type OpenValue } from "../value/Value";

const open = (address: Address) => void useStore.getState().openAddress(formatAddress(address));

const rowTableOf = (address: Address | null) => (address && "table" in address.target ? address.target.table : undefined);

const plural = (n: number, one: string, many: string) => `${formatCount(n)} ${n === 1 ? one : many}`;

type Collapse = ReturnType<typeof useCollapsedGroups>;

/** Loads once per `dataVersion`, and again whenever the tab is shown, since recent objects change without one. */
function useReadout(tab: ReadoutTab) {
  const entry = useStore((s) => s.readouts[readoutKey(tab.sessionId, tab.database)]);
  const dataVersion = useStore((s) => s.dataVersion);
  const shown = useStore((s) => s.activeTabId === tab.id);
  useEffect(() => {
    if (shown) void useStore.getState().loadReadout(tab.sessionId, tab.database, true);
  }, [tab.sessionId, tab.database, shown]);
  // `dataVersion` is only a refetch trigger: the effect reads nothing from it.
  useEffect(() => {
    void useStore.getState().loadReadout(tab.sessionId, tab.database);
  }, [tab.sessionId, tab.database, dataVersion]);
  return entry;
}

interface SectionProps {
  id: string;
  label: string;
  name: string;
  count?: string;
  hint?: string;
  collapse: Collapse;
  children: ReactNode;
}

/** A section band as in the object view: chevron, name, hint, count; collapsed per database like a document group. */
function Section({ id, label, name, count, hint, collapse, children }: SectionProps) {
  const collapsed = collapse.isCollapsed(id);
  const toggleLabel = `${collapsed ? "Expand" : "Collapse"} ${name}`;
  const onBandClick = (e: MouseEvent) => {
    if (!(e.target as HTMLElement).closest("button")) collapse.toggle(id);
  };
  return (
    <section aria-label={label} className="readout-section">
      <div className="sh" onClick={onBandClick}>
        <button className="sh-toggle" aria-expanded={!collapsed} aria-label={toggleLabel} data-stage={`readout:collapse:${id}`} data-stage-label={toggleLabel} onClick={() => collapse.toggle(id)}>
          {collapsed ? <ChevronRight size="0.923rem" /> : <ChevronDown size="0.923rem" />}
        </button>
        <span className="tn sans">{name}</span>
        {hint && <span className="via">{hint}</span>}
        <span className="spacer" />
        {count && <span className="cnt">{count}</span>}
      </div>
      {!collapsed && children}
    </section>
  );
}

/** An error in place of what failed: a row of its own (`row`), or text after the line's content. */
function LineError({ error, row = false }: { error: string | null; row?: boolean }) {
  if (!error) return null;
  return row ? <div className="row line-error">{error}</div> : <span className="line-error"> · {error}</span>;
}

function StatusHeader({ status, onOpen }: { status: Status; onOpen: OpenValue }) {
  const meta = [status.host, status.prefix, status.presets.join(", "), status.serverVersion].filter(Boolean);
  const values = status.values.items;
  return (
    <section aria-label="Status" className="readout-head">
      <h1 className="readout-title">{status.database}</h1>
      <div className="readout-meta">
        {meta.map((part, i) => (
          <span key={i}>
            {i > 0 && <span className="o3"> · </span>}
            <span>{part}</span>
          </span>
        ))}
      </div>
      {values.length > 0 && (
        <div className="readout-table readout-facts">
          {values.map((v) => {
            const table = v.value?.source?.table;
            return (
              <div key={v.id} className="row" data-row-table={table}>
                <span className="o2">
                  {table && <span className="o3">{table} </span>}
                  <button className="line-open" data-stage={`readout:value:${v.id}`} data-stage-label={v.title} title={v.sql} onClick={() => open(v.address)}>
                    {v.title}
                  </button>
                </span>
                <span>{v.error ? <LineError error={v.error} /> : v.value ? <Value value={v.value} density="grid" onOpen={onOpen} /> : <span className="o3">no row</span>}</span>
              </div>
            );
          })}
        </div>
      )}
      <LineError error={status.values.error} />
    </section>
  );
}

function openAnomaly(line: AnomalyLine) {
  const s = useStore.getState();
  const address = line.address;
  if (!address) return;
  if (address.target.kind === "table" && line.filter) return s.openTableTab(address.database, address.target.table, "data", line.filter);
  open(address);
}

function AnomalyRow({ line }: { line: AnomalyLine }) {
  return (
    <div className="row" data-row-table={rowTableOf(line.address)}>
      <span className="r">{line.count ?? ""}</span>
      <span title={`${line.preset} · ${line.id}`}>
        {line.text}
        <LineError error={line.error} />
      </span>
      {line.fix ? (
        <button
          className="line-open act"
          data-stage={`readout:fix:${line.id}`}
          data-stage-label={line.fix.action}
          onClick={() => line.fix && void useStore.getState().queueAction(line.fix.address, line.fix.action, {})}
        >
          {line.fix.action}
        </button>
      ) : (
        <span />
      )}
      {line.address ? (
        <button className="line-open o3" data-stage={`readout:anomaly:${line.id}`} data-stage-label={line.text} onClick={() => openAnomaly(line)}>
          → rows
        </button>
      ) : (
        <span />
      )}
    </div>
  );
}

function Anomalies({ part, collapse }: { part: Part<AnomalyLine>; collapse: Collapse }) {
  if (part.items.length === 0 && !part.error) return null;
  return (
    <Section id="anomalies" label="Anomalies" name="anomalies" hint="preset rules, shown while the count is above 0" count={plural(part.items.length, "rule", "rules")} collapse={collapse}>
      <LineError error={part.error} row />
      <div className="readout-table readout-anomalies">
        {part.items.map((line) => (
          <AnomalyRow key={`${line.preset}:${line.id}`} line={line} />
        ))}
      </div>
    </Section>
  );
}

/** A header row of column names, then the rows; a row of a known table carries it for the linked table highlight. */
function RowsTable({ columns, rows, table, onOpen }: { columns: string[]; rows: ReadoutRow[]; table?: string; onOpen: OpenValue }) {
  const gridTemplateColumns = `repeat(${Math.max(columns.length, 1)}, minmax(0, max-content)) 1fr`;
  return (
    <div className="readout-table" style={{ gridTemplateColumns }}>
      <div className="row hdr">
        {columns.map((c) => (
          <span key={c}>{c}</span>
        ))}
      </div>
      {rows.map((row, i) => (
        <div key={row.address ? formatAddress(row.address) : i} className="row" data-row-table={rowTableOf(row.address) ?? table}>
          {row.values.map((value: StoredValue, col) => (
            <span key={col}>
              <Value value={value} density="grid" onOpen={onOpen} />
            </span>
          ))}
        </div>
      ))}
    </div>
  );
}

interface BlockProps {
  anchor?: string;
  label: string;
  title: ReactNode;
  detail?: ReactNode;
  sql: string;
  children: ReactNode;
}

/** One entity or pinned query: its heading, the SQL behind it in tertiary text, then its rows. */
function Block({ anchor, label, title, detail, sql, children }: BlockProps) {
  return (
    <div className="readout-block" data-stage={anchor} data-stage-label={anchor && label}>
      <div className="readout-block-head">
        {title}
        {detail}
      </div>
      <div className="readout-sql">{sql}</div>
      {children}
    </div>
  );
}

function NewSection({ part, database, collapse, onOpen }: { part: Part<NewObjects>; database: string; collapse: Collapse; onOpen: OpenValue }) {
  if (part.items.length === 0 && !part.error) return null;
  return (
    <Section id="new" label="New objects" name="new" hint="the newest rows of each entity with a created column" count={plural(part.items.length, "entity", "entities")} collapse={collapse}>
      <LineError error={part.error} row />
      <div className="readout-blocks">
        {part.items.map((objects) => (
          <Block
            key={`${objects.preset}:${objects.entity}:${objects.table}`}
            anchor={`readout:new:${objects.entity}`}
            label={objects.entity}
            title={<span className="readout-block-title">{objects.entity}</span>}
            detail={
              <>
                <button className="line-open o2" data-table={objects.table} title="Open the table" onClick={() => useStore.getState().openTableTab(database, objects.table, "data")}>
                  {objects.table}
                </button>
                <span className="o3">{plural(objects.rows.length, "row", "rows")}</span>
              </>
            }
            sql={objects.sql}
          >
            {objects.error ? <LineError error={objects.error} row /> : <RowsTable columns={objects.columns} rows={objects.rows} table={objects.table} onOpen={onOpen} />}
          </Block>
        ))}
      </div>
    </Section>
  );
}

function PinnedTitle({ line }: { line: PinnedLine }) {
  return (
    <button className="line-open" data-stage={`readout:pinned:${line.id}`} data-stage-label={line.title} onClick={() => open(line.address)}>
      {line.title}
    </button>
  );
}

function PinnedSection({ part, collapse, onOpen }: { part: Part<PinnedLine>; collapse: Collapse; onOpen: OpenValue }) {
  const counts = part.items.filter((line) => line.result?.show !== "rows");
  const blocks = part.items.flatMap((line) => (line.result?.show === "rows" ? [{ line, result: line.result }] : []));
  return (
    <Section id="pinned" label="Pinned queries" name="pinned" hint="p pins a query in any SQL tab · counts refresh after every apply" count={plural(part.items.length, "query", "queries")} collapse={collapse}>
      <LineError error={part.error} row />
      {part.items.length === 0 && !part.error && <div className="row readout-empty">no pinned queries</div>}
      {counts.length > 0 && (
        <div className="readout-table readout-pinned">
          {counts.map((line) => (
            <div key={line.id} className="row">
              <span className="r">{line.result?.show === "count" ? line.result.count : ""}</span>
              <span>
                <PinnedTitle line={line} />
                <LineError error={line.error} />
              </span>
              <span className="o3" title={line.sql}>
                {line.sql}
              </span>
            </div>
          ))}
        </div>
      )}
      {blocks.length > 0 && (
        <div className="readout-blocks wide">
          {blocks.map(({ line, result }) => (
            <Block key={line.id} label={line.title} title={<PinnedTitle line={line} />} detail={<span className="o3">{plural(result.rows.length, "row", "rows")}</span>} sql={line.sql}>
              <RowsTable columns={result.columns} rows={result.rows} onOpen={onOpen} />
            </Block>
          ))}
        </div>
      )}
    </Section>
  );
}

function RecentSection({ part, collapse, onOpen }: { part: Part<RecentLine>; collapse: Collapse; onOpen: OpenValue }) {
  const rows = part.items.flatMap((line) => (line.address.target.kind === "row" ? [{ line, target: line.address.target }] : []));
  if (rows.length === 0 && !part.error) return null;
  return (
    <Section id="recent" label="Recent" name="recent" hint="objects opened lately" count={plural(rows.length, "object", "objects")} collapse={collapse}>
      <LineError error={part.error} row />
      <div className="readout-table readout-recent">
        {rows.map(({ line, target }) => {
          const text = formatAddress(line.address);
          return (
            <div key={text} className="row" data-row-table={target.table}>
              <span className="o2">{target.table}</span>
              <button className="line-open key" data-stage={`readout:recent:${text}`} data-stage-label={`${target.table} #${target.key}`} onClick={() => open(line.address)}>
                #{target.key}
              </button>
              <span className="readout-summary">
                {line.error ? (
                  <LineError error={line.error} />
                ) : (
                  line.summary.map(({ column, value }) => (
                    <span key={column} title={column}>
                      <Value value={value} density="grid" onOpen={onOpen} />
                    </span>
                  ))
                )}
              </span>
              <span className="o3 r" title={`opened ${new Date(line.at * 1000).toISOString()}`}>
                {relativeTime(line.at)}
              </span>
            </div>
          );
        })}
      </div>
    </Section>
  );
}

export function Readout({ tab }: { tab: ReadoutTab }) {
  const entry = useReadout(tab);
  const { openValue, targetMenu } = useOpenValue(null);
  const collapse = useCollapsedGroups(tab.address);
  const readout = entry?.readout;
  if (entry?.error && !readout) return <ErrorPanel message={entry.error.message} code={entry.error.code} />;
  if (!readout) return <div className="empty-state">Loading…</div>;
  return (
    <div className="readout" data-stage="readout" data-stage-label={`readout · ${tab.database}`}>
      <StatusHeader status={readout.status} onOpen={openValue} />
      <Anomalies part={readout.anomalies} collapse={collapse} />
      <NewSection part={readout.newObjects} database={tab.database} collapse={collapse} onOpen={openValue} />
      <PinnedSection part={readout.pinned} collapse={collapse} onOpen={openValue} />
      <RecentSection part={readout.recent} collapse={collapse} onOpen={openValue} />
      {targetMenu}
    </div>
  );
}
