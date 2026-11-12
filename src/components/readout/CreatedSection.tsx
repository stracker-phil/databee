import { useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import type { CreatedEntity, CreatedItem, RecentlyCreated, Value as StoredValue } from "../../api";
import { formatAddress } from "../../lib/address";
import { cellText, plural, relativeTime } from "../../lib/format";
import { sourceText } from "../../lib/provenance";
import { useStore } from "../../state/store";
import { TableIcon } from "../TableIcon";
import { SummaryLine } from "../value/SummaryLine";
import type { OpenValue } from "../value/Value";
import { LineError, openAddress, Section, type Collapse } from "./Section";

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
      <button aria-pressed={shown} data-table={entity.table} data-stage={`readout:entity:${name}`} data-stage-label={name} title={entity.error ?? `${entity.table} · preset ${entity.preset}`} onClick={onToggle}>
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
      <button className="line-open key" data-stage={`readout:created:${text}`} data-stage-label={`${item.entity} #${key}`} onClick={() => openAddress(item.address)}>
        <TableIcon database={item.address.database} table={item.table} effect="go" />
        #{key}
      </button>
      <SummaryLine summary={item.summary} onOpen={onOpen} />
    </div>
  );
}

interface EntityChipsProps {
  entities: CreatedEntity[];
  hidden: ReadonlySet<string>;
  details: string | null;
  withTable: (entity: { entity: string }) => boolean;
  onToggle: (key: string) => void;
  onDetails: (key: string) => void;
}

function EntityChips({ entities, hidden, details, withTable, onToggle, onDetails }: EntityChipsProps) {
  return (
    <div className="entity-chips" role="group" aria-label="Entities">
      {entities.map((entity) => {
        const key = entityKey(entity);
        return (
          <EntityChip
            key={`${entity.preset}:${key}`}
            entity={entity}
            withTable={withTable(entity)}
            shown={!hidden.has(key)}
            open={details === key}
            onToggle={() => onToggle(key)}
            onDetails={() => onDetails(key)}
          />
        );
      })}
    </div>
  );
}

function CreatedRows({ items, withTable, onOpen }: { items: CreatedItem[]; withTable: (item: { entity: string }) => boolean; onOpen: OpenValue }) {
  if (items.length === 0) return <div className="row readout-empty">no rows</div>;
  return (
    <div className="readout-table readout-created">
      {items.map((item) => (
        <CreatedLine key={formatAddress(item.address)} item={item} withTable={withTable(item)} onOpen={onOpen} />
      ))}
    </div>
  );
}

export function CreatedSection({ part, database, collapse, onOpen }: { part: RecentlyCreated; database: string; collapse: Collapse; onOpen: OpenValue }) {
  const [hidden, setHidden] = useState<ReadonlySet<string>>(new Set());
  const [details, setDetails] = useState<string | null>(null);
  if (part.entities.length === 0 && !part.error) return null;
  const toggle = (key: string) => {
    const next = new Set(hidden);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    setHidden(next);
  };
  const items = part.items.filter((item) => !hidden.has(entityKey(item)));
  const detailed = part.entities.find((e) => entityKey(e) === details);
  const sameName = (entity: { entity: string }) => part.entities.filter((e) => e.entity === entity.entity).length > 1;
  return (
    <Section id="created" label="Recently created" name="recently created" hint="newest rows of every entity with a created column" count={plural(items.length, "row", "rows")} collapse={collapse}>
      <LineError error={part.error} row />
      <EntityChips entities={part.entities} hidden={hidden} details={details} withTable={sameName} onToggle={toggle} onDetails={(key) => setDetails(details === key ? null : key)} />
      {detailed && <EntityQuery entity={detailed} database={database} />}
      <CreatedRows items={items} withTable={sameName} onOpen={onOpen} />
    </Section>
  );
}
