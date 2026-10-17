import type { ColumnKindRule, ColumnStyleRule, KindRow, ProjectChange, ReferenceRow, StyleRow } from "../../api";
import { kindFormFromRow, kindText, referenceFormFromRow, ruleOriginText, scopeText, styleFormFromRow, styleMatchText } from "../../lib/ruleForms";
import { fail } from "../../state/store";
import type { ProjectContext } from "../project/context";
import { CoverageCell, coverageKey, type CoverageState } from "../project/coverage";
import { ActionButtons, columnsText, referenceLabel, RuleSwitch } from "../project/ReferenceLines";

type Actions = [string, () => void][];

/** The page's rule context; `null` until the project's rules are loaded, so the controls wait. */
export type RuleControls = ProjectContext | null;

async function send(ctx: ProjectContext, change: ProjectChange, context: string) {
  const failed = await ctx.change(change);
  if (failed) fail(failed, context);
}

function Origin({ origin, edited, pattern }: { origin: ReferenceRow["origin"]; edited?: boolean; pattern?: string | null }) {
  return (
    <span className="origin">
      {ruleOriginText(origin)}
      {pattern && <span className="mono">: {pattern}</span>}
      {edited && " · edited in project"}
    </span>
  );
}

function referenceActions(row: ReferenceRow, ctx: ProjectContext): Actions {
  const id = row.id;
  if (!id) return [];
  const actions: Actions = [];
  const form = referenceFormFromRow(row, `edit ${id}`);
  if (form) actions.push(["edit", () => ctx.setForm(form)]);
  if (row.edited) actions.push(["reset", () => void send(ctx, { op: "removeReference", id }, "Could not reset the reference")]);
  else if (row.origin.kind === "project") actions.push(["remove", () => void send(ctx, { op: "removeReference", id }, "Could not remove the reference")]);
  return actions;
}

export function referenceKey(row: ReferenceRow) {
  return `${columnsText(row.from)}${row.when}->${columnsText(row.to)}${row.id ? `#${row.id}` : ""}`;
}

const rowsWith = (condition: string) => condition && <span className="o3"> (rows with {condition})</span>;
const thisRowHas = (condition: string) => condition && <span className="o3"> when this row has {condition}</span>;

/**
 * Keeps the two ends' conditions apart: `when this row has …` is on the row the line belongs to, `(rows with …)`
 * on the rows at the other end; a column that references itself reads `other <table> rows with the same <column>`.
 */
function ReferenceText({ row, shows, onOpenTable }: Pick<ReferenceItemProps, "row" | "shows" | "onOpenTable">) {
  const link = (end: ReferenceRow["to"], text: string) => (
    <button className="link" onClick={() => onOpenTable(end.table)}>
      {text}
    </button>
  );
  if (shows === "source")
    return (
      <>
        ← {link(row.from, columnsText(row.from))}
        {rowsWith(row.when)}
        {thisRowHas(row.toWhen)}
      </>
    );
  if (columnsText(row.from) === columnsText(row.to))
    return (
      <>
        → other {link(row.to, row.to.table)} rows with the same {row.to.columns.join(", ")}
        {row.toWhen && ` and ${row.toWhen}`}
        {thisRowHas(row.when)}
      </>
    );
  return (
    <>
      → {link(row.to, columnsText(row.to))}
      {rowsWith(row.toWhen)}
      {thisRowHas(row.when)}
    </>
  );
}

interface ReferenceItemProps {
  row: ReferenceRow;
  coverage: Record<string, CoverageState>;
  ctx: RuleControls;
  /** What the line names first: the target (a column's reference) or the source (an incoming one). */
  shows: "target" | "source";
  onOpenTable: (table: string) => void;
}

export function ReferenceItem({ row, coverage, ctx, shows, onOpenTable }: ReferenceItemProps) {
  const label = referenceLabel(row);
  const anchor = `structure:reference:${referenceKey(row)}`;
  const id = row.id;
  const toggle = (on: boolean) => {
    if (ctx && id) void send(ctx, { op: on ? "enable" : "disable", id }, `Could not switch ${label}`);
  };
  return (
    <div className={`rule-item${row.disabled ? " off" : ""}`} role="group" aria-label={label} data-stage={anchor} data-stage-label={label}>
      <span className={`rule-main${row.origin.kind === "inferred" ? " dashed" : ""}`}>
        <ReferenceText row={row} shows={shows} onOpenTable={onOpenTable} />
      </span>
      <span className="rule-meta">
        <Origin origin={row.origin} edited={row.edited} />
        {row.reference ? <CoverageCell state={coverage[coverageKey(row.reference)]} /> : <span className="o3">{row.disabled ? "off" : "not applied"}</span>}
        {ctx && id && <RuleSwitch on={!row.disabled} label={label} anchor={`${anchor}:switch`} onChange={toggle} />}
        {ctx && <ActionButtons anchor={anchor} actions={referenceActions(row, ctx)} />}
      </span>
    </div>
  );
}

function kindActions(row: KindRow, ctx: ProjectContext): Actions {
  const index = row.index;
  if (index === null) return [];
  return [
    ["edit", () => ctx.setForm(kindFormFromRow(row))],
    ["remove", () => void send(ctx, { op: "removeKind", index }, "Could not remove the kind")],
  ];
}

export function KindItem({ row, pattern, applies = true, ctx }: { row: KindRow; pattern?: string | null; applies?: boolean; ctx: RuleControls }) {
  const text = kindText(row.rule);
  const anchor = `structure:kind:${row.index ?? ruleOriginText(row.origin)}:${scopeText(row.rule)}`;
  return (
    <div className={`rule-item${applies ? "" : " off"}`} role="group" aria-label={`kind ${text}`} data-stage={anchor}>
      <span className="rule-main">
        {text}
        {!applies && <span className="o3"> · overridden</span>}
      </span>
      <span className="rule-meta">
        <Origin origin={row.origin} pattern={pattern} />
        {ctx && <ActionButtons anchor={anchor} actions={kindActions(row, ctx)} />}
      </span>
    </div>
  );
}

export const columnKindItem = (kind: ColumnKindRule, ctx: RuleControls) => <KindItem key={`${kind.row.index ?? ruleOriginText(kind.row.origin)}:${scopeText(kind.row.rule)}`} row={kind.row} pattern={kind.pattern} applies={kind.applies} ctx={ctx} />;

function styleActions(row: StyleRow, ctx: ProjectContext): Actions {
  const id = row.id;
  const actions: Actions = [["edit", () => ctx.setForm(styleFormFromRow(row))]];
  if (row.edited) actions.push(["reset", () => void send(ctx, { op: "removeStyle", id }, "Could not reset the value style")]);
  else if (row.origin.kind === "project") actions.push(["remove", () => void send(ctx, { op: "removeStyle", id }, "Could not remove the value style")]);
  return actions;
}

export function StyleItem({ row, pattern, ctx }: { row: StyleRow; pattern?: string | null; ctx: RuleControls }) {
  const { rule } = row;
  const match = styleMatchText(rule);
  const label = `style ${match} ${rule.color}`;
  const anchor = `structure:style:${row.id}`;
  const toggle = (on: boolean) => {
    if (ctx) void send(ctx, { op: on ? "enableStyle" : "disableStyle", id: row.id }, `Could not switch the value style ${row.id}`);
  };
  return (
    <div className={`rule-item${row.disabled ? " off" : ""}`} role="group" aria-label={label} data-stage={anchor}>
      <span className="rule-main">
        {match} <span className="o3">→</span> <span style={{ color: `var(--style-${rule.color})` }}>{rule.color}</span>
      </span>
      <span className="rule-meta">
        <Origin origin={row.origin} edited={row.edited} pattern={pattern} />
        {ctx && <RuleSwitch on={!row.disabled} label={label} anchor={`${anchor}:switch`} onChange={toggle} />}
        {ctx && <ActionButtons anchor={anchor} actions={styleActions(row, ctx)} />}
      </span>
    </div>
  );
}

export const columnStyleItem = (style: ColumnStyleRule, ctx: RuleControls) => <StyleItem key={`${ruleOriginText(style.row.origin)}:${style.row.id}`} row={style.row} pattern={style.pattern} ctx={ctx} />;

