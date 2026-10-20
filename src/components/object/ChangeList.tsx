import type { Cell, Provenance } from "../../api";
import { useBindingLabel } from "../../commands";
import { clockTime, sinceText, type ChangeLine, type ChangeMarks } from "../../lib/diff";
import { formatCount } from "../../lib/format";
import { oldText } from "./groups";
import { WideTable } from "./WideTable";

const LINE_WIDTHS = ["10ch", "32ch", "32ch", "20ch"];

function before(line: ChangeLine): string {
  if (line.kind === "added") return "no row";
  return line.old ? oldText(line.old) : "row";
}

function after(line: ChangeLine): string {
  if (line.kind === "removed") return "no row";
  return line.new ? oldText(line.new) : "new row";
}

const text = (cell: Cell | null) => (cell ? oldText(cell) : "");

interface Props {
  marks: ChangeMarks;
  /** The command that dismisses the marks (`object.dismissChanges`, `query.dismissChanges`). */
  dismissCommand: string;
  onJump: (source: Provenance) => void;
}

export function ChangeList({ marks, dismissCommand, onJump }: Props) {
  const dismissKeys = useBindingLabel(dismissCommand);
  if (marks.lines.length === 0) return null;
  const hint = [sinceText(marks.since), "in key order within one poll", dismissKeys ? `${dismissKeys} dismisses` : null].filter(Boolean).join(" · ");
  return (
    <section className="changes" aria-label="Changes" data-stage="changes" data-stage-label="Changes">
      <div className="sh">
        <span className="tn sans">changes</span>
        <span className="via">{hint}</span>
        <span className="spacer" />
        <span className="cnt">{formatCount(marks.lines.length)}</span>
      </div>
      <WideTable widths={LINE_WIDTHS}>
        <div className="row hdr">
          <span>time</span>
          <span>table</span>
          <span>column or key</span>
          <span>old → new</span>
        </div>
        {marks.lines.map((line, i) => (
          <button
            key={i}
            className="row change-line"
            disabled={!line.source}
            title={line.kind === "changed" ? `${text(line.old)} → ${text(line.new)}` : undefined}
            data-stage={`change:${i}`}
            data-stage-label={`${line.table} ${line.label}`}
            data-stage-list="changes"
            onClick={() => line.source && onJump(line.source)}
          >
            <span className="num">{clockTime(line.at)}</span>
            <span className="o2">{line.table}</span>
            <span>{line.label}</span>
            <span>
              <span className="o3">{before(line)}</span> → {after(line)}
            </span>
          </button>
        ))}
      </WideTable>
    </section>
  );
}
