import type { AnomalyLine, Part } from "../../api";
import { plural } from "../../lib/format";
import { useStore } from "../../state/store";
import { runTableAction } from "../object/actions";
import { OpenHint } from "../value/OpenHint";
import { LineError, openAddress, rowTableOf, Section, type Collapse } from "./Section";

function openAnomaly(line: AnomalyLine) {
  const address = line.address;
  if (!address) return;
  if (address.target.kind === "table" && line.filter) return useStore.getState().openTableTab(address.database, address.target.table, "data", line.filter);
  openAddress(address);
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

export function Anomalies({ part, collapse }: { part: Part<AnomalyLine>; collapse: Collapse }) {
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
