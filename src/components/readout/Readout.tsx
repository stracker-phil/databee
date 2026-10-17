/** The readout tab (`databee://<slug>/<db>/`): status line, anomalies, new, pinned and recent; every line opens its address. */
import { useEffect, type ReactNode } from "react";
import type { Address, AnomalyLine, NewObjects, Part, PinnedLine, ReadoutRow, RecentLine, Status, Value as StoredValue } from "../../api";
import { formatAddress } from "../../lib/address";
import { relativeTime } from "../../lib/format";
import { readoutKey } from "../../state/slices/readout";
import { useStore, type ReadoutTab } from "../../state/store";
import { ErrorPanel } from "../ErrorPanel";
import { useOpenValue } from "../value/useOpenValue";
import { Value, type OpenValue } from "../value/Value";

const open = (address: Address) => void useStore.getState().openAddress(formatAddress(address));

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

function Band({ name, note }: { name: string; note?: string }) {
  return (
    <div className="sh">
      <span className="tn sans">{name}</span>
      {note && <span className="via">{note}</span>}
    </div>
  );
}

/** An error in place of what failed: a row of its own (`row`), or text after the line's content. */
function LineError({ error, row = false }: { error: string | null; row?: boolean }) {
  if (!error) return null;
  return row ? <div className="row line-error">{error}</div> : <span className="line-error"> · {error}</span>;
}

function StatusLine({ status, onOpen }: { status: Status; onOpen: OpenValue }) {
  const parts: ReactNode[] = [
    <span key="host">{status.host}</span>,
    <span key="database">{status.database}</span>,
    status.prefix && <span key="prefix">{status.prefix}</span>,
    status.presets.length > 0 && <span key="presets" className="o2">{status.presets.join(", ")}</span>,
    <span key="server" className="o2">{status.serverVersion}</span>,
    ...status.values.items.map((v) => (
      <span key={`value:${v.id}`}>
        <button className="line-open o2" data-stage={`readout:value:${v.id}`} data-stage-label={v.title} title={v.sql} onClick={() => open(v.address)}>
          {v.title}
        </button>{" "}
        {v.error ? <LineError error={v.error} /> : v.value ? <Value value={v.value} density="grid" onOpen={onOpen} /> : <span className="o3">no row</span>}
      </span>
    )),
  ].filter(Boolean);
  return (
    <section aria-label="Status" className="readout-status">
      {parts.map((part, i) => (
        <span key={i} className="readout-status-part">
          {i > 0 && <span className="o3">·</span>}
          {part}
        </span>
      ))}
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
    <div className="row anomaly">
      <span className="r">{line.count ?? ""}</span>
      <span title={`${line.preset} · ${line.id}`}>
        {line.text}
        <LineError error={line.error} />
      </span>
      {line.fix ? (
        <button
          className="line-open o3"
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

function Anomalies({ part }: { part: Part<AnomalyLine> }) {
  if (part.items.length === 0 && !part.error) return null;
  return (
    <section aria-label="Anomalies">
      <Band name="anomalies" note="preset rules, shown while the count is above 0" />
      <LineError error={part.error} row />
      {part.items.map((line) => (
        <AnomalyRow key={`${line.preset}:${line.id}`} line={line} />
      ))}
    </section>
  );
}

function Rows({ columns, rows, onOpen }: { columns: string[]; rows: ReadoutRow[]; onOpen: OpenValue }) {
  const gridTemplateColumns = `repeat(${Math.max(columns.length, 1)}, minmax(0, max-content))`;
  return (
    <>
      <div className="row hdr" style={{ gridTemplateColumns }}>
        {columns.map((c) => (
          <span key={c}>{c}</span>
        ))}
      </div>
      {rows.map((row, i) => (
        <div key={row.address ? formatAddress(row.address) : i} className="row" style={{ gridTemplateColumns }}>
          {row.values.map((value: StoredValue, col) => (
            <span key={col}>
              <Value value={value} density="grid" onOpen={onOpen} />
            </span>
          ))}
        </div>
      ))}
    </>
  );
}

function NewSection({ part, onOpen }: { part: Part<NewObjects>; onOpen: OpenValue }) {
  if (part.items.length === 0 && !part.error) return null;
  return (
    <section aria-label="New objects">
      <Band name="new" note="the newest rows of each entity with a created column" />
      <LineError error={part.error} row />
      {part.items.map((objects) => (
        <div key={`${objects.preset}:${objects.entity}:${objects.table}`} className="readout-block" data-stage={`readout:new:${objects.entity}`} data-stage-label={objects.entity}>
          {objects.error ? <LineError error={objects.error} row /> : <Rows columns={objects.columns} rows={objects.rows} onOpen={onOpen} />}
          <div className="readout-sql">
            {objects.sql} · {objects.rows.length} rows · {objects.entity}
          </div>
        </div>
      ))}
    </section>
  );
}

function PinnedSection({ part, onOpen }: { part: Part<PinnedLine>; onOpen: OpenValue }) {
  return (
    <section aria-label="Pinned queries">
      <Band name="pinned" />
      <LineError error={part.error} row />
      {part.items.map((line) => {
        const result = line.result;
        const title = (
          <button className="line-open" data-stage={`readout:pinned:${line.id}`} data-stage-label={line.title} onClick={() => open(line.address)}>
            {line.title}
          </button>
        );
        if (result?.show === "rows") {
          return (
            <div key={line.id} className="readout-block">
              <div className="row pinned-title">{title}</div>
              <Rows columns={result.columns} rows={result.rows} onOpen={onOpen} />
              <div className="readout-sql">{line.sql}</div>
            </div>
          );
        }
        return (
          <div key={line.id} className="row pinned">
            <span className="r">{result?.show === "count" ? result.count : ""}</span>
            <span>
              {title}
              <LineError error={line.error} />
            </span>
            <span className="o3">{line.sql}</span>
          </div>
        );
      })}
      <div className="readout-hint">p pins a query in any SQL tab · counts refresh after every apply</div>
    </section>
  );
}

function RecentSection({ part, onOpen }: { part: Part<RecentLine>; onOpen: OpenValue }) {
  const rows = part.items.flatMap((line) => (line.address.target.kind === "row" ? [{ line, target: line.address.target }] : []));
  if (rows.length === 0 && !part.error) return null;
  return (
    <section aria-label="Recent">
      <Band name="recent" />
      <LineError error={part.error} row />
      {rows.map(({ line, target }) => {
        const text = formatAddress(line.address);
        return (
          <div key={text} className="row recent">
            <span className="o2">{target.table}</span>
            <button className="line-open key" data-stage={`readout:recent:${text}`} data-stage-label={`${target.table} #${target.key}`} onClick={() => open(line.address)}>
              #{target.key}
            </button>
            <span className="recent-summary">
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
            <span className="o3" title={`opened ${new Date(line.at * 1000).toISOString()}`}>
              {relativeTime(line.at)}
            </span>
          </div>
        );
      })}
    </section>
  );
}

export function Readout({ tab }: { tab: ReadoutTab }) {
  const entry = useReadout(tab);
  const { openValue, targetMenu } = useOpenValue(null);
  const readout = entry?.readout;
  if (entry?.error && !readout) return <ErrorPanel message={entry.error.message} code={entry.error.code} />;
  if (!readout) return <div className="empty-state">Loading…</div>;
  return (
    <div className="readout" data-stage="readout" data-stage-label={`readout · ${tab.database}`}>
      <div className="readout-col">
        <StatusLine status={readout.status} onOpen={openValue} />
        <Anomalies part={readout.anomalies} />
        <NewSection part={readout.newObjects} onOpen={openValue} />
        <PinnedSection part={readout.pinned} onOpen={openValue} />
        <RecentSection part={readout.recent} onOpen={openValue} />
      </div>
      {targetMenu}
    </div>
  );
}
