import type { ProjectChange, ReferencedBy, ReferenceRow } from "../../api";
import { referenceFormFromRow, ruleOriginText } from "../../lib/ruleForms";
import { fail } from "../../state/store";
import { LongList } from "../LongList";
import type { ProjectContext } from "./context";
import { CoverageCell, coverageKey, type CoverageState } from "./coverage";

export const columnsText = (ref: { table: string; columns: string[] }) => `${ref.table}.${ref.columns.join(", ")}`;

export const referenceLabel = (row: { from: ReferenceRow["from"]; to: ReferenceRow["to"]; when: string }) => `${columnsText(row.from)}${row.when ? ` ${row.when}` : ""} → ${columnsText(row.to)}`;

const referenceKey = (row: ReferenceRow) => `${columnsText(row.from)}->${columnsText(row.to)}${row.id ? `#${row.id}` : ""}`;

async function send(ctx: ProjectContext, change: ProjectChange, context: string) {
  const failed = await ctx.change(change);
  if (failed) fail(failed, context);
}

export function RuleSwitch({ on, label, anchor, onChange }: { on: boolean; label: string; anchor: string; onChange: (on: boolean) => void }) {
  return <input className="switch small" type="checkbox" role="switch" aria-label={label} data-stage={anchor} data-stage-label={label} checked={on} onChange={(e) => onChange(e.target.checked)} />;
}

export function ActionButtons({ anchor, actions }: { anchor: string; actions: [string, () => void][] }) {
  return (
    <span className="acts">
      {actions.map(([name, run], i) => (
        <span key={name}>
          {i > 0 && <span className="o3"> · </span>}
          <button className="act" data-stage={`${anchor}:${name}`} onClick={run}>
            {name}
          </button>
        </span>
      ))}
    </span>
  );
}

function referenceActions(row: ReferenceRow, ctx: ProjectContext): [string, () => void][] {
  const id = row.id;
  if (!id) return [];
  const actions: [string, () => void][] = [];
  const form = referenceFormFromRow(row, `edit ${id}`);
  if (form) actions.push(["edit", () => ctx.setForm(form)]);
  if (row.edited) actions.push(["reset", () => void send(ctx, { op: "removeReference", id }, "Could not reset the reference")]);
  else if (row.origin.kind === "project") actions.push(["remove", () => void send(ctx, { op: "removeReference", id }, "Could not remove the reference")]);
  return actions;
}

function rowClass(row: ReferenceRow): string {
  if (row.disabled) return "row refs off";
  return row.origin.kind === "project" || row.edited ? "row refs" : "row refs o2";
}

function ReferenceLine({ row, coverage, ctx }: { row: ReferenceRow; coverage: CoverageState | undefined; ctx: ProjectContext }) {
  const anchor = `reference:${referenceKey(row)}`;
  const label = referenceLabel(row);
  const id = row.id;
  const toggle = (on: boolean) => {
    if (id) void send(ctx, { op: on ? "enable" : "disable", id }, `Could not switch ${label}`);
  };
  return (
    <div className={rowClass(row)} role="row" aria-label={label} data-stage={anchor} data-stage-label={columnsText(row.from)} data-stage-list="references">
      <span className="src">
        {row.from.columns.join(", ")}
        {row.when && <span className="o3"> when {row.when}</span>}
      </span>
      <span className={`src${row.origin.kind === "inferred" ? " dashed" : ""}`}>
        → {columnsText(row.to)}
        {row.toWhen && <span className="o3"> where {row.toWhen}</span>}
      </span>
      <span className="origin">
        {ruleOriginText(row.origin)}
        {row.edited && <span className="o3"> · edited in project</span>}
      </span>
      <span className="r">{row.reference ? <CoverageCell state={coverage} /> : <span className="o3">{row.disabled ? "off" : "not applied"}</span>}</span>
      <span className="r">{id ? <RuleSwitch on={!row.disabled} label={label} anchor={`${anchor}:switch`} onChange={toggle} /> : null}</span>
      <ActionButtons anchor={anchor} actions={referenceActions(row, ctx)} />
    </div>
  );
}

export function ReferenceLines({ rows, coverage, ctx }: { rows: ReferenceRow[]; coverage: Record<string, CoverageState>; ctx: ProjectContext }) {
  return (
    <LongList items={rows}>
      {(row) => <ReferenceLine key={referenceKey(row)} row={row} coverage={row.reference ? coverage[coverageKey(row.reference)] : undefined} ctx={ctx} />}
    </LongList>
  );
}

/** The tables pointing here, one line: `referenced by wp_wc_order_items.order_id, …`. */
export function ReferencedByLine({ incoming }: { incoming: ReferencedBy[] }) {
  if (incoming.length === 0) return null;
  return (
    <div className="row referenced-by o2" role="row" aria-label="referenced by">
      <span>
        <span className="o3">referenced by </span>
        {incoming.map((r, i) => (
          <span key={`${columnsText(r.from)}${r.when}->${columnsText(r.to)}`} className={r.disabled ? "off" : undefined} title={`${referenceLabel(r)}${r.disabled ? " · off" : ""}`}>
            {i > 0 && ", "}
            {columnsText(r.from)}
            {r.when && <span className="o3"> when {r.when}</span>}
          </span>
        ))}
      </span>
    </div>
  );
}
