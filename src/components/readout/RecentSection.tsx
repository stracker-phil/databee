import type { Part, RecentLine } from "../../api";
import { formatAddress } from "../../lib/address";
import { plural, relativeTime } from "../../lib/format";
import { TableIcon } from "../TableIcon";
import { SummaryLine } from "../value/SummaryLine";
import type { OpenValue } from "../value/Value";
import { LineError, openAddress, Section, type Collapse } from "./Section";

export function RecentSection({ part, collapse, onOpen }: { part: Part<RecentLine>; collapse: Collapse; onOpen: OpenValue }) {
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
              <button className="line-open key" data-stage={`readout:recent:${text}`} data-stage-label={`${target.table} #${target.key}`} onClick={() => openAddress(line.address)}>
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
