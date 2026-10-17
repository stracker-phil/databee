import { useState } from "react";
import type { ProjectChange, StyleColor } from "../../api";
import { fieldError, KIND_CHOICES, kindRule, STYLE_COLORS, STYLE_MATCHES, styleRule, ZONES, type FieldError, type KindChoice, type KindForm as KForm, type ScopeDraft, type StyleForm as SForm } from "../../lib/ruleForms";
import type { ProjectContext } from "./context";
import { FieldMessage, FormFrame, Labeled } from "./formParts";

/** Saves through `ctx.change`; a field error stays with the form, success closes it. */
function useSave(ctx: ProjectContext) {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<FieldError | null>(null);
  const save = async (change: ProjectChange) => {
    setSaving(true);
    const failed = await ctx.change(change);
    setSaving(false);
    if (failed) {
      setError(fieldError(failed));
      return;
    }
    ctx.setForm(null);
  };
  return { saving, error, setError, save };
}

function ScopeFields({ scope, error, onChange }: { scope: ScopeDraft; error: FieldError | null; onChange: (scope: ScopeDraft) => void }) {
  const field = (key: keyof ScopeDraft, label: string, placeholder: string) => (
    <Labeled label={label}>
      <input className="input mono" aria-label={label} data-stage={`project:form:${key}`} placeholder={placeholder} value={scope[key]} onChange={(e) => onChange({ ...scope, [key]: e.target.value })} />
      <FieldMessage error={error} field={key} />
    </Labeled>
  );
  return (
    <>
      {field("columns", "columns", "wc_orders.status, *.post_status")}
      {field("metaKeys", "meta keys", "_payment_method, _wc_*")}
      {field("inMeta", "in meta tables", "wc_orders_meta")}
    </>
  );
}

export function KindForm({ form, ctx }: { form: KForm; ctx: ProjectContext }) {
  const { saving, error, setError, save } = useSave(ctx);
  const update = (patch: Partial<KForm>) => {
    setError(null);
    ctx.setForm({ ...form, ...patch });
  };
  const rule = kindRule(form);
  const change: ProjectChange = form.index === null ? { op: "addKind", rule } : { op: "updateKind", index: form.index, rule };
  return (
    <FormFrame title={form.index === null ? "new kind" : "edit kind"} opener={form.opener} anchor="project:form" saving={saving} error={error} onCancel={() => ctx.setForm(null)} onSave={() => void save(change)} footer={null}>
      <ScopeFields scope={form.scope} error={error} onChange={(scope) => update({ scope })} />
      <Labeled label="kind">
        <span className="picker">
          <select className="input mono" aria-label="Kind" data-stage="project:form:kind" value={form.kind} onChange={(e) => update({ kind: e.target.value as KindChoice })}>
            {KIND_CHOICES.map((k) => (
              <option key={k} value={k}>
                {k}
              </option>
            ))}
          </select>
          {form.kind === "timestamp" && (
            <select className="input mono" aria-label="Zone" data-stage="project:form:zone" value={form.zone} onChange={(e) => update({ zone: e.target.value as KForm["zone"] })}>
              {ZONES.map((z) => (
                <option key={z} value={z}>
                  {z}
                </option>
              ))}
            </select>
          )}
          {form.kind === "money" && (
            <input className="input mono" aria-label="Currency from" data-stage="project:form:currency" placeholder="currency from (columns or meta keys)" value={form.currency} onChange={(e) => update({ currency: e.target.value })} />
          )}
        </span>
      </Labeled>
    </FormFrame>
  );
}

function ColorChoice({ color, chosen, onPick }: { color: StyleColor; chosen: boolean; onPick: () => void }) {
  return (
    <button type="button" className="swatch-choice" aria-label={color} aria-pressed={chosen} data-stage={`project:form:color:${color}`} data-stage-list="project:form:color" onClick={onPick}>
      <span className="swatch" style={{ background: `var(--style-${color})` }} />
      <span style={{ color: `var(--style-${color})` }}>{color}</span>
    </button>
  );
}

export function StyleForm({ form, ctx }: { form: SForm; ctx: ProjectContext }) {
  const { saving, error, setError, save } = useSave(ctx);
  const update = (patch: Partial<SForm>) => {
    setError(null);
    ctx.setForm({ ...form, ...patch });
  };
  const rule = styleRule(form);
  const change: ProjectChange = form.index === null ? { op: "addStyle", rule } : { op: "updateStyle", index: form.index, rule };
  return (
    <FormFrame title={form.index === null ? "new value style" : "edit value style"} opener={form.opener} anchor="project:form" saving={saving} error={error} onCancel={() => ctx.setForm(null)} onSave={() => void save(change)} footer={null}>
      <ScopeFields scope={form.scope} error={error} onChange={(scope) => update({ scope })} />
      <Labeled label="match">
        <span className="picker">
          <select className="input mono" aria-label="Match" data-stage="project:form:match" value={form.match} onChange={(e) => update({ match: e.target.value as SForm["match"] })}>
            {STYLE_MATCHES.map((m) => (
              <option key={m} value={m}>
                {m}
              </option>
            ))}
          </select>
          <input className="input mono" aria-label="Value" data-stage="project:form:value" value={form.value} onChange={(e) => update({ value: e.target.value })} />
        </span>
        <FieldMessage error={error} field="value" />
      </Labeled>
      <Labeled label="color">
        <span className="swatches" role="group" aria-label="Color">
          {STYLE_COLORS.map((c) => (
            <ColorChoice key={c} color={c} chosen={form.color === c} onPick={() => update({ color: c })} />
          ))}
        </span>
      </Labeled>
    </FormFrame>
  );
}
