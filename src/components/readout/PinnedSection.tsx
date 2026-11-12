import { useMemo } from "react";
import type { Part, PinnedLine, PinnedResult, ReadoutRow, Value as StoredValue } from "../../api";
import { formatAddress } from "../../lib/address";
import { cellText, charWidth, plural } from "../../lib/format";
import { LONG_LIST, LongList } from "../LongList";
import { OpenHint } from "../value/OpenHint";
import { Value, type OpenValue } from "../value/Value";
import { LineError, openAddress, rowTableOf, Section, type Collapse } from "./Section";

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

function PinnedTitle({ line }: { line: PinnedLine }) {
  return (
    <button className="line-open" data-table={line.tables[0]} data-stage={`readout:pinned:${line.id}`} data-stage-label={line.title} onClick={() => openAddress(line.address)}>
      {line.title}
      <OpenHint effect="go" />
    </button>
  );
}

function PinnedCounts({ lines }: { lines: PinnedLine[] }) {
  return (
    <div className="readout-table readout-pinned">
      {lines.map((line) => (
        <div key={line.id} className="row">
          <span className="r">{line.result?.show === "count" ? line.result.count : ""}</span>
          <span>
            <PinnedTitle line={line} />
            <LineError error={line.error} />
          </span>
          <span className="o3" data-tooltip={line.sql}>
            {line.sql}
          </span>
        </div>
      ))}
    </div>
  );
}

function PinnedBlocks({ blocks, onOpen }: { blocks: { line: PinnedLine; result: Extract<PinnedResult, { show: "rows" }> }[]; onOpen: OpenValue }) {
  return (
    <div className="readout-blocks wide">
      {blocks.map(({ line, result }) => (
        <div key={line.id} className="readout-block">
          <div className="readout-block-head">
            <PinnedTitle line={line} />
            <span className="o3">{plural(result.rows.length, "row", "rows")}</span>
          </div>
          <div className="readout-sql">{line.sql}</div>
          <RowsTable columns={result.columns} rows={result.rows} onOpen={onOpen} />
        </div>
      ))}
    </div>
  );
}

export function PinnedSection({ part, collapse, onOpen }: { part: Part<PinnedLine>; collapse: Collapse; onOpen: OpenValue }) {
  const counts = part.items.filter((line) => line.result?.show !== "rows");
  const blocks = part.items.flatMap((line) => (line.result?.show === "rows" ? [{ line, result: line.result }] : []));
  return (
    <Section id="pinned" label="Pinned queries" name="pinned" hint="p pins a query in any SQL tab · counts refresh after every apply" count={plural(part.items.length, "query", "queries")} collapse={collapse}>
      <LineError error={part.error} row />
      {part.items.length === 0 && !part.error && <div className="row readout-empty">no pinned queries</div>}
      {counts.length > 0 && <PinnedCounts lines={counts} />}
      {blocks.length > 0 && <PinnedBlocks blocks={blocks} onOpen={onOpen} />}
    </Section>
  );
}
