import type { Summary } from "../../api";
import { Value, type OpenValue } from "./Value";

/** A row's summary (`summary.rs`) as `name value` pairs, the same in the header, peek, readout and palette. */
export function SummaryLine({ summary, onOpen, className = "" }: { summary: Summary; onOpen?: OpenValue; className?: string }) {
  if (summary.values.length === 0) return null;
  return (
    <span className={`summary-line ${className}`.trim()}>
      {summary.values.map(({ name, value }) => (
        <span key={name} className="summary-pair">
          <span className="summary-name">{name}</span>
          <Value value={value} density="grid" onOpen={onOpen} />
        </span>
      ))}
    </span>
  );
}
