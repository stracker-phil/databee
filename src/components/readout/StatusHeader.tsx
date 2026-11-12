import type { Status, Value as StoredValue } from "../../api";
import { TableIcon } from "../TableIcon";
import { OpenHint } from "../value/OpenHint";
import { Value, type OpenValue } from "../value/Value";
import { LineError, openAddress } from "./Section";

function FactValue({ value, error, onOpen }: { value: StoredValue | null; error: string | null; onOpen: OpenValue }) {
  if (error) return <LineError error={error} />;
  if (!value) return <span className="o3">no row</span>;
  return <Value value={value} density="grid" onOpen={onOpen} />;
}

export function StatusHeader({ status, onOpen }: { status: Status; onOpen: OpenValue }) {
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
                <button className="line-open" data-stage={`readout:value:${v.id}`} data-stage-label={v.title} data-tooltip={v.sql} onClick={() => openAddress(v.address)}>
                  {v.title}
                  <OpenHint effect="go" />
                </button>
                <span>
                  <FactValue value={v.value} error={v.error} onOpen={onOpen} />
                </span>
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
