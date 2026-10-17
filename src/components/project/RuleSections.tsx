import type { ActionLine, EntityLine, KindRow, ProjectChange, StyleRow } from "../../api";
import { kindFormFromRow, kindText, ruleOriginText, scopeText, styleFormFromRow, styleMatchText } from "../../lib/ruleForms";
import { fail } from "../../state/store";
import { LongList } from "../LongList";
import type { ProjectContext } from "./context";
import { ActionButtons, RuleSwitch } from "./ReferenceLines";

async function send(ctx: ProjectContext, change: ProjectChange, context: string) {
  const failed = await ctx.change(change);
  if (failed) fail(failed, context);
}

function styleActions(row: StyleRow, ctx: ProjectContext): [string, () => void][] {
  const id = row.id;
  const actions: [string, () => void][] = [["edit", () => ctx.setForm(styleFormFromRow(row))]];
  if (row.edited) actions.push(["reset", () => void send(ctx, { op: "removeStyle", id }, "Could not reset the value style")]);
  else if (row.origin.kind === "project") actions.push(["remove", () => void send(ctx, { op: "removeStyle", id }, "Could not remove the value style")]);
  return actions;
}

function StyleLine({ row, ctx }: { row: StyleRow; ctx: ProjectContext }) {
  const { rule } = row;
  const own = row.origin.kind === "project" || row.edited;
  const anchor = `style:${row.id}`;
  const label = `${scopeText(rule)} ${styleMatchText(rule)}`;
  const toggle = (on: boolean) => void send(ctx, { op: on ? "enableStyle" : "disableStyle", id: row.id }, `Could not switch the value style ${row.id}`);
  return (
    <div className={`row styles${own ? "" : " o2"}${row.disabled ? " off" : ""}`} role="row" aria-label={label} data-stage={anchor} data-stage-list="styles">
      <span className="src">{scopeText(rule)}</span>
      <span>{styleMatchText(rule)}</span>
      <span style={{ color: `var(--style-${rule.color})` }}>{rule.color}</span>
      <span className="origin">
        {ruleOriginText(row.origin)}
        {row.edited && <span className="o3"> · edited in project</span>}
      </span>
      <span className="r">
        <RuleSwitch on={!row.disabled} label={label} anchor={`${anchor}:switch`} onChange={toggle} />
      </span>
      <ActionButtons anchor={anchor} actions={styleActions(row, ctx)} />
    </div>
  );
}

export function StyleLines({ rows, ctx }: { rows: StyleRow[]; ctx: ProjectContext }) {
  return <LongList items={rows}>{(row) => <StyleLine key={`${ruleOriginText(row.origin)}:${row.id}`} row={row} ctx={ctx} />}</LongList>;
}

function KindLine({ row, ctx }: { row: KindRow; ctx: ProjectContext }) {
  const { rule, index } = row;
  const own = index !== null;
  const anchor = `kind:${own ? index : `${ruleOriginText(row.origin)}:${scopeText(rule)}`}`;
  const actions: [string, () => void][] = own
    ? [
        ["edit", () => ctx.setForm(kindFormFromRow(row))],
        ["remove", () => void send(ctx, { op: "removeKind", index }, "Could not remove the kind")],
      ]
    : [];
  return (
    <div className={`row kinds${own ? "" : " o2"}`} role="row" aria-label={`${scopeText(rule)} ${kindText(rule)}`} data-stage={anchor} data-stage-list="kinds">
      <span className="src">{scopeText(rule)}</span>
      <span>{kindText(rule)}</span>
      <span className="origin">{ruleOriginText(row.origin)}</span>
      <ActionButtons anchor={anchor} actions={actions} />
    </div>
  );
}

export function KindLines({ rows, ctx }: { rows: KindRow[]; ctx: ProjectContext }) {
  return <LongList items={rows}>{(row, i) => <KindLine key={`${row.index ?? ruleOriginText(row.origin)}:${scopeText(row.rule)}:${i}`} row={row} ctx={ctx} />}</LongList>;
}

/** Read-only: `entity order · preset woocommerce`, `action mark_paid · row · preset woocommerce`. */
export function EntityActionLines({ entities, actions }: { entities: EntityLine[]; actions: ActionLine[] }) {
  return (
    <>
      {entities.map((e) => (
        <div key={`entity:${e.preset}:${e.name}:${e.filter ?? ""}`} className="row facts o2" role="row" aria-label={`entity ${e.name}`}>
          <span>
            <span className="o3">entity </span>
            {e.name}
            {e.filter && <span className="o3"> where {e.filter}</span>}
            <span className="o3"> · preset {e.preset}</span>
          </span>
        </div>
      ))}
      {actions.map((a) => (
        <div key={`action:${a.preset}:${a.name}`} className="row facts o2" role="row" aria-label={`action ${a.name}`}>
          <span>
            <span className="o3">action </span>
            {a.name}
            <span className="o3">
              {" "}
              · {a.scope} · preset {a.preset}
            </span>
          </span>
        </div>
      ))}
    </>
  );
}
