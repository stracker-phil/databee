import { useState } from "react";
import { api, type Coverage, type Reference } from "../../api";
import { fieldError, referenceComplete, referenceRule, type FieldError, type ReferenceForm as Form } from "../../lib/ruleForms";
import { useApiResult } from "../../lib/useApiResult";
import { useTableColumns, ColumnPicker, FieldMessage, FormFrame, Labeled } from "./formParts";
import type { ProjectTab } from "../../state/store";
import type { ProjectContext } from "./context";
import { CoverageCounts, nothingResolves } from "./coverage";

const PREVIEW_DELAY_MS = 400;

type Preview = { status: "idle" } | { status: "running" } | { status: "done"; coverage: Coverage } | { status: "failed"; error: FieldError };

/** Coverage of the rule as typed, debounced; Rust checks it like a save, so errors name the field. */
function useCoveragePreview(tab: ProjectTab, rule: Reference | null, replacing: string | null): Preview {
  const { sessionId, database } = tab;
  const key = rule ? JSON.stringify([sessionId, database, rule, replacing]) : null;
  const result = useApiResult(key, () => api.coveragePreview(sessionId, database, rule!, replacing), { delayMs: PREVIEW_DELAY_MS });
  if (key === null) return { status: "idle" };
  if (result.state === "failed") return { status: "failed", error: fieldError(result.error) };
  if (result.state === "loaded") return { status: "done", coverage: result.data };
  return { status: "running" };
}

function PreviewLine({ preview }: { preview: Preview }) {
  if (preview.status === "idle") return <span className="o3">pick a source and a target to see the coverage</span>;
  if (preview.status === "running") return <span className="o3">checking…</span>;
  if (preview.status === "failed") return null;
  const coverage = preview.coverage;
  return (
    <span className="coverage-line" aria-label="Coverage">
      <CoverageCounts coverage={coverage} suffix=" resolve" />
      {nothingResolves(coverage) && (
        <span className="warning" role="alert">
          no source value matches a target; the rule is probably wrong
          {coverage.sampleUnresolved.length > 0 && <> · unresolved: <span className="mono">{coverage.sampleUnresolved.join(", ")}</span></>}
        </span>
      )}
    </span>
  );
}

function WhenField({ ctx, form, update }: { ctx: ProjectContext; form: Form; update: (patch: Partial<Form>) => void }) {
  const { columns } = useTableColumns(ctx.tab.sessionId, ctx.tab.database, form.source.table);
  if (form.whenLocked) return <span className="mono o2">{Object.keys(form.base?.when ?? {}).join(", ")} (kept as stored)</span>;
  const when = form.when ?? { column: "", value: "" };
  return (
    <span className="picker">
      <select className="input mono" aria-label="When column" data-stage="project:form:when-column" value={when.column} onChange={(e) => update({ when: e.target.value ? { ...when, column: e.target.value } : null })}>
        <option value="">always</option>
        {when.column && !columns.includes(when.column) && <option value={when.column}>{when.column}</option>}
        {columns.map((c) => (
          <option key={c} value={c}>
            {c}
          </option>
        ))}
      </select>
      {when.column && (
        <>
          <span className="o3">=</span>
          <input className="input mono" aria-label="When value" data-stage="project:form:when-value" value={when.value} onChange={(e) => update({ when: { ...when, value: e.target.value } })} />
        </>
      )}
    </span>
  );
}

export function ReferenceForm({ form, ctx }: { form: Form; ctx: ProjectContext }) {
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<FieldError | null>(null);
  const { tables } = ctx.rules;
  const { sessionId, database } = ctx.tab;
  const update = (patch: Partial<Form>) => {
    setSaveError(null);
    ctx.setForm({ ...form, ...patch });
  };
  const rule = referenceComplete(form) ? referenceRule(form, tables) : null;
  const preview = useCoveragePreview(ctx.tab, rule, form.replacing);
  const error = saveError ?? (preview.status === "failed" ? preview.error : null);

  const save = async () => {
    if (!rule) {
      setSaveError({ field: "target", message: "pick a target table and column" });
      return;
    }
    setSaving(true);
    const change = form.replacing ? { op: "updateReference" as const, id: form.replacing, reference: rule } : { op: "addReference" as const, reference: rule };
    const failed = await ctx.change(change);
    setSaving(false);
    if (failed) {
      setSaveError(fieldError(failed));
      return;
    }
    ctx.setForm(null);
  };

  return (
    <FormFrame
      title={form.replacing ? "edit reference" : "new reference"}
      opener={form.opener}
      anchor="project:form"
      saving={saving}
      error={error}
      onCancel={() => ctx.setForm(null)}
      onSave={() => void save()}
      footer={<PreviewLine preview={preview} />}
    >
      <Labeled label="source">
        <ColumnPicker label="Source" anchor="project:form:source" sessionId={sessionId} database={database} tables={tables} value={form.source} onChange={(source) => update({ source })} />
        <FieldMessage error={error} field="source" />
      </Labeled>
      <Labeled label="when">
        <WhenField ctx={ctx} form={form} update={update} />
        <FieldMessage error={error} field="when" />
      </Labeled>
      <Labeled label="target">
        <ColumnPicker label="Target" anchor="project:form:target" sessionId={sessionId} database={database} tables={tables} value={form.target} onChange={(target) => update({ target })} preferKey />
        <FieldMessage error={error} field="target" />
      </Labeled>
      <Labeled label="label">
        <input className="input mono" aria-label="Label" data-stage="project:form:label" placeholder="names the incoming group" value={form.label} onChange={(e) => update({ label: e.target.value })} />
        <FieldMessage error={error} field="label" />
      </Labeled>
    </FormFrame>
  );
}
