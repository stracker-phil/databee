import { useEffect, useMemo, useState, type MouseEvent, type ReactNode } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import type { Address, AnomalyLine, CreatedEntity, CreatedItem, Part, PinnedLine, ReadoutRow, RecentLine, RecentlyCreated, Status, Value as StoredValue } from "../../api";
import { formatAddress } from "../../lib/address";
import { cellText, charWidth, plural, relativeTime } from "../../lib/format";
import { sourceText } from "../../lib/provenance";
import { readoutKey } from "../../state/slices/readout";
import { useStore, type ReadoutTab } from "../../state/store";
import { ErrorPanel } from "../ErrorPanel";
import { LONG_LIST, LongList } from "../LongList";
import { runTableAction } from "../object/actions";
import { useCollapsedGroups } from "../object/useCollapsedGroups";
import { TableIcon } from "../TableIcon";
import { OpenHint } from "../value/OpenHint";
import { useOpenValue } from "../value/useOpenValue";
import { SummaryLine } from "../value/SummaryLine";
import { Value, type OpenValue } from "../value/Value";

const open = (address: Address) => void useStore.getState().openAddress(formatAddress(address));

const rowTableOf = (address: Address | null) => (address && "table" in address.target ? address.target.table : undefined);

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
      <div className="readout-meta">{meta.join(" · ")}</div>
      {values.length > 0 && (
        <div className="readout-table readout-facts">
          {values.map((v) => {
            const table = v.value?.source?.table;
            return (
              <div key={v.id} className="row" data-row-table={table}>
                <button className="line-open" data-stage={`readout:value:${v.id}`} data-stage-label={v.title} title={v.sql} onClick={() => open(v.address)}>
                  {v.title}
                  <OpenHint effect="go" />
                </button>
                <span>{v.error ? <LineError error={v.error} /> : v.value ? <Value value={v.value} density="grid" onOpen={onOpen} /> : <span className="o3">no row</span>}</span>
                <span className="o3">
                  <TableIcon database={status.database} table={table} />
                  {table}
                </span>
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
    <div className="attention" data-row-table={rowTableOf(line.address)}>
      <span className="attention-count">{line.count ?? "–"}</span>
      <span className="attention-text">
        <span>{line.text}</span>
        <span className="o3">
          {line.preset} · {line.id}
        </span>
        <LineError error={line.error} />
      </span>
      <span className="attention-actions">
        {line.address && (
          <button className="btn" data-stage={`readout:anomaly:${line.id}`} data-stage-label={line.text} onClick={() => openAnomaly(line)}>
            Show rows
            <OpenHint effect="go" />
          </button>
        )}
        {line.fix && (
          <button
            className="btn mono"
            title={`Plan ${line.fix.action} on ${rowTableOf(line.fix.address) ?? ""}`}
            data-stage={`readout:fix:${line.id}`}
            data-stage-label={line.fix.action}
            onClick={() => line.fix && void runTableAction(line.fix.address, line.fix.action)}
          >
            {line.fix.action}
          </button>
        )}
      </span>
    </div>
  );
}

function Anomalies({ part, collapse }: { part: Part<AnomalyLine>; collapse: Collapse }) {
  if (part.items.length === 0 && !part.error) return null;
  return (
    <Section id="anomalies" label="Needs attention" name="needs attention" hint="preset anomalies whose count is above 0" count={plural(part.items.length, "rule", "rules")} collapse={collapse}>
      <LineError error={part.error} row />
      <div className="attention-list">
        {part.items.map((line) => (
          <AnomalyRow key={`${line.preset}:${line.id}`} line={line} />
        ))}
      </div>
    </Section>
  );
}

/** Columns fit their content; a windowed table sizes them from every row instead, as only some are rendered. */
function tableTemplate(columns: string[], rows: ReadoutRow[]): string {
  if (rows.length <= LONG_LIST) return `repeat(${Math.max(columns.length, 1)}, minmax(0, max-content)) 1fr`;
  const widths = columns.map((column, col) => charWidth(column, rows.map((row) => (row.values[col] ? cellText(row.values[col].raw) : ""))));
  return `${widths.join(" ")} 1fr`;
}

/** A header row of column names, then the rows; a row of a known table carries it for the linked table highlight. */
function RowsTable({ columns, rows, table, onOpen }: { columns: string[]; rows: ReadoutRow[]; table?: string; onOpen: OpenValue }) {
  const gridTemplateColumns = useMemo(() => tableTemplate(columns, rows), [columns, rows]);
  return (
    <div className="readout-table" style={{ gridTemplateColumns }}>
      <div className="row hdr">
        {columns.map((c) => (
          <span key={c}>{c}</span>
        ))}
      </div>
      <LongList items={rows}>
        {(row, i) => (
          <div key={row.address ? formatAddress(row.address) : i} className="row" data-row-table={rowTableOf(row.address) ?? table}>
            {row.values.map((value: StoredValue, col) => (
              <span key={col}>
                <Value value={value} density="grid" onOpen={onOpen} />
              </span>
            ))}
          </div>
        )}
      </LongList>
    </div>
  );
}

interface BlockProps {
  title: ReactNode;
  detail?: ReactNode;
  sql: string;
  children: ReactNode;
}

function Block({ title, detail, sql, children }: BlockProps) {
  return (
    <div className="readout-block">
      <div className="readout-block-head">
        {title}
        {detail}
      </div>
      <div className="readout-sql">{sql}</div>
      {children}
    </div>
  );
}

function createdTime(created: StoredValue): { text: string; title: string } {
  const source = created.source ? sourceText(created.source) : "created";
  const stored = cellText(created.raw);
  const utc = created.kind.kind === "timestamp" ? created.kind.utc : null;
  if (utc === null) return { text: stored, title: source };
  return { text: relativeTime(utc), title: `${stored} · ${source}` };
}

const entityKey = (entity: { entity: string; table: string }) => `${entity.entity} ${entity.table}`;

interface ChipProps {
  entity: CreatedEntity;
  withTable: boolean;
  shown: boolean;
  open: boolean;
  onToggle: () => void;
  onDetails: () => void;
}

function EntityChip({ entity, withTable, shown, open, onToggle, onDetails }: ChipProps) {
  const name = withTable ? entityKey(entity) : entity.entity;
  return (
    <span className={`entity-chip${shown ? " on" : ""}${entity.error ? " failed" : ""}`}>
      <button aria-pressed={shown} data-stage={`readout:entity:${name}`} data-stage-label={name} title={entity.error ?? `${entity.table} · preset ${entity.preset}`} onClick={onToggle}>
        <span>{entity.entity}</span>
        {withTable && <span className="tbl">{entity.table}</span>}{" "}
        <span className="cnt">{entity.error ? "failed" : entity.countInWindow}</span>
      </button>
      <button className="entity-chip-more" aria-expanded={open} aria-label={`Query of ${name}`} onClick={onDetails}>
        {open ? <ChevronDown size="0.846rem" /> : <ChevronRight size="0.846rem" />}
      </button>
    </span>
  );
}

function EntityQuery({ entity, database }: { entity: CreatedEntity; database: string }) {
  return (
    <div className="entity-query" aria-label={`${entity.entity} query`}>
      <div>
        <span className="o2">{entity.entity}</span>
        <span className="o3"> · </span>
        <button className="line-open" data-table={entity.table} title="Open the table" onClick={() => useStore.getState().openTableTab(database, entity.table, "data")}>
          <TableIcon database={database} table={entity.table} effect="go" />
          {entity.table}
        </button>
        <span className="o3"> · preset {entity.preset}</span>
      </div>
      <div className="readout-sql">{entity.sql}</div>
      <LineError error={entity.error} />
    </div>
  );
}

function CreatedLine({ item, withTable, onOpen }: { item: CreatedItem; withTable: boolean; onOpen: OpenValue }) {
  const time = createdTime(item.created);
  const text = formatAddress(item.address);
  const key = item.address.target.kind === "row" ? item.address.target.key : "";
  return (
    <div className="row" data-row-table={item.table}>
      <span className="o3 r" title={time.title}>
        {time.text}
      </span>
      <span className="o2">
        {item.entity}
        {withTable && <span className="o3"> {item.table}</span>}
      </span>
      <button className="line-open key" data-stage={`readout:created:${text}`} data-stage-label={`${item.entity} #${key}`} onClick={() => open(item.address)}>
        <TableIcon database={item.address.database} table={item.table} effect="go" />
        #{key}
      </button>
      <SummaryLine summary={item.summary} onOpen={onOpen} />
    </div>
  );
}

function CreatedSection({ part, database, collapse, onOpen }: { part: RecentlyCreated; database: string; collapse: Collapse; onOpen: OpenValue }) {
  const [hidden, setHidden] = useState<ReadonlySet<string>>(new Set());
  const [details, setDetails] = useState<string | null>(null);
  if (part.entities.length === 0 && !part.error) return null;
  const toggle = (key: string) => {
    const next = new Set(hidden);
    if (!next.delete(key)) next.add(key);
    setHidden(next);
  };
  const items = part.items.filter((item) => !hidden.has(entityKey(item)));
  const detailed = part.entities.find((e) => entityKey(e) === details);
  const sameName = (entity: { entity: string }) => part.entities.filter((e) => e.entity === entity.entity).length > 1;
  return (
    <Section id="created" label="Recently created" name="recently created" hint="newest rows of every entity with a created column" count={plural(items.length, "row", "rows")} collapse={collapse}>
      <LineError error={part.error} row />
      <div className="entity-chips" role="group" aria-label="Entities">
        {part.entities.map((entity) => {
          const key = entityKey(entity);
          return (
            <EntityChip
              key={`${entity.preset}:${key}`}
              entity={entity}
              withTable={sameName(entity)}
              shown={!hidden.has(key)}
              open={details === key}
              onToggle={() => toggle(key)}
              onDetails={() => setDetails(details === key ? null : key)}
            />
          );
        })}
      </div>
      {detailed && <EntityQuery entity={detailed} database={database} />}
      {items.length === 0 ? (
        <div className="row readout-empty">no rows</div>
      ) : (
        <div className="readout-table readout-created">
          {items.map((item) => (
            <CreatedLine key={formatAddress(item.address)} item={item} withTable={sameName(item)} onOpen={onOpen} />
          ))}
        </div>
      )}
    </Section>
  );
}

function PinnedTitle({ line }: { line: PinnedLine }) {
  return (
    <button className="line-open" data-stage={`readout:pinned:${line.id}`} data-stage-label={line.title} onClick={() => open(line.address)}>
      {line.title}
      <OpenHint effect="go" />
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
            <Block key={line.id} title={<PinnedTitle line={line} />} detail={<span className="o3">{plural(result.rows.length, "row", "rows")}</span>} sql={line.sql}>
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
    <Section id="recent" label="Recently opened" name="recently opened" hint="rows opened in this project" count={plural(rows.length, "object", "objects")} collapse={collapse}>
      <LineError error={part.error} row />
      <div className="readout-table readout-recent">
        {rows.map(({ line, target }) => {
          const text = formatAddress(line.address);
          return (
            <div key={text} className="row" data-row-table={target.table}>
              <span className="o2">{target.table}</span>
              <button className="line-open key" data-stage={`readout:recent:${text}`} data-stage-label={`${target.table} #${target.key}`} onClick={() => open(line.address)}>
                <TableIcon database={line.address.database} table={target.table} effect="go" />
                #{target.key}
              </button>
              <span>{line.error ? <span className="line-error">{line.error}</span> : <SummaryLine summary={line.summary} onOpen={onOpen} />}</span>
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
      <CreatedSection part={readout.recentlyCreated} database={tab.database} collapse={collapse} onOpen={openValue} />
      <PinnedSection part={readout.pinned} collapse={collapse} onOpen={openValue} />
      <RecentSection part={readout.recent} collapse={collapse} onOpen={openValue} />
      {targetMenu}
    </div>
  );
}
