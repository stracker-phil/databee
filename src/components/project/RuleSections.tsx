/** Value styles and kinds: project rules editable, preset rules read-only in `--o2`. */
import type { KindRow, ProjectChange, StyleRow } from "../../api";
import { kindFormFromRow, kindText, originText, scopeText, styleFormFromRow, styleMatchText } from "../../lib/ruleForms";
import { fail } from "../../state/store";
import type { ProjectContext } from "./context";

function RowActions({ anchor, onEdit, change, ctx }: { anchor: string; onEdit(): void; change: ProjectChange; ctx: ProjectContext }) {
  const remove = async () => {
    const failed = await ctx.change(change);
    if (failed) fail(failed, "Could not remove the rule");
  };
  return (
    <span className="acts">
      <button className="act" data-stage={`${anchor}:edit`} onClick={onEdit}>
        edit
      </button>
      <span className="o3"> · </span>
      <button className="act" data-stage={`${anchor}:remove`} onClick={() => void remove()}>
        remove
      </button>
    </span>
  );
}

function StyleLine({ row, ctx }: { row: StyleRow; ctx: ProjectContext }) {
  const { rule, index } = row;
  const own = index !== null;
  const anchor = `style:${own ? index : `${originText(row.origin)}:${scopeText(rule)}:${rule.value}`}`;
  return (
    <div className={`row styles${own ? "" : " o2"}${rule.enabled ? "" : " off"}`} role="row" aria-label={`${scopeText(rule)} ${styleMatchText(rule)}`} data-stage={anchor} data-stage-list="styles">
      <span className="src">{scopeText(rule)}</span>
      <span>{styleMatchText(rule)}</span>
      <span style={{ color: `var(--style-${rule.color})` }}>{rule.color}</span>
      <span>
        {originText(row.origin)}
        {!rule.enabled && <span className="o3"> · off</span>}
      </span>
      {own ? <RowActions anchor={anchor} ctx={ctx} onEdit={() => ctx.setForm(styleFormFromRow(row))} change={{ op: "removeStyle", index }} /> : <SwitchOn row={row} anchor={anchor} ctx={ctx} />}
    </div>
  );
}

/** A preset ships its styles off; switching one on copies it into the project. */
function SwitchOn({ row, anchor, ctx }: { row: StyleRow; anchor: string; ctx: ProjectContext }) {
  if (row.rule.enabled) return <span />;
  const add = async () => {
    const failed = await ctx.change({ op: "addStyle", rule: { ...row.rule, enabled: true } });
    if (failed) fail(failed, "Could not switch the style on");
  };
  return (
    <span className="acts">
      <button className="act" data-stage={`${anchor}:on`} onClick={() => void add()}>
        switch on
      </button>
    </span>
  );
}

export function StylesSection({ ctx }: { ctx: ProjectContext }) {
  const styles = ctx.rules.styles;
  return (
    <section aria-label="Value styles">
      <div className="sh">
        <span className="tn sans">value styles</span>
        <span className="via">color by value, scoped to columns or meta keys; none by default; colors come from the scheme</span>
      </div>
      <div className="row styles hdr" role="row">
        <span>column or meta key</span>
        <span>match</span>
        <span>color</span>
        <span>origin</span>
        <span />
      </div>
      {styles.length === 0 && <div className="row empty o3">no value styles; add one with "Style…" on a value</div>}
      {styles.map((row, i) => (
        <StyleLine key={i} row={row} ctx={ctx} />
      ))}
    </section>
  );
}

function KindLine({ row, ctx }: { row: KindRow; ctx: ProjectContext }) {
  const { rule, index } = row;
  const own = index !== null;
  const anchor = `kind:${own ? index : `${originText(row.origin)}:${scopeText(rule)}`}`;
  return (
    <div className={`row kinds${own ? "" : " o2"}`} role="row" aria-label={`${scopeText(rule)} ${kindText(rule)}`} data-stage={anchor} data-stage-list="kinds">
      <span className="src">{scopeText(rule)}</span>
      <span>{kindText(rule)}</span>
      <span>{originText(row.origin)}</span>
      {own ? <RowActions anchor={anchor} ctx={ctx} onEdit={() => ctx.setForm(kindFormFromRow(row))} change={{ op: "removeKind", index }} /> : <span />}
    </div>
  );
}

export function KindsSection({ ctx }: { ctx: ProjectContext }) {
  const kinds = ctx.rules.kinds;
  return (
    <section aria-label="Kinds">
      <div className="sh">
        <span className="tn sans">kinds</span>
        <span className="via">what a column or meta value holds</span>
      </div>
      <div className="row kinds hdr" role="row">
        <span>column or meta key</span>
        <span>kind</span>
        <span>origin</span>
        <span />
      </div>
      {kinds.length === 0 && <div className="row empty o3">no kinds</div>}
      {kinds.map((row, i) => (
        <KindLine key={i} row={row} ctx={ctx} />
      ))}
    </section>
  );
}
