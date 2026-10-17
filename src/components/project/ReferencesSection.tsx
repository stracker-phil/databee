import { useState } from "react";
import { api, toAppError, type ProjectChange, type ReferenceRow } from "../../api";
import { originText, referenceFormFromRow } from "../../lib/ruleForms";
import { fail } from "../../state/store";
import { LongList } from "../LongList";
import type { ProjectContext } from "./context";
import { CoverageCell, type CoverageState } from "./coverage";

const columnsText = (ref: { table: string; columns: string[] }) => `${ref.table}.${ref.columns.join(", ")}`;

export const referenceKey = (row: ReferenceRow) => `${originText(row.origin)}:${columnsText(row.from)}->${columnsText(row.to)}${row.id ? `#${row.id}` : ""}`;

function rowClass(row: ReferenceRow): string {
  if (row.disabled) return "row refs off";
  if (row.origin.kind === "project") return "row refs";
  return "row refs o2";
}

function OriginCell({ row }: { row: ReferenceRow }) {
  const origin = row.origin;
  if (origin.kind === "inferred") {
    return (
      <span>
        inferred <span className="o3">same name · {origin.indexed ? "indexed" : "not indexed"}</span>
        {row.disabled && <span className="o3"> · disabled</span>}
      </span>
    );
  }
  return (
    <span>
      {originText(origin)}
      {row.disabled && <span className="o3"> · disabled</span>}
    </span>
  );
}

type ActionName = "check" | "edit" | "remove" | "enable" | "promote" | "disable";

function rowActions(row: ReferenceRow): ActionName[] {
  const actions: ActionName[] = row.reference ? ["check"] : [];
  if (!row.id) return actions;
  if (row.origin.kind === "project") return [...actions, "edit", "remove"];
  if (row.disabled) return [...actions, "enable"];
  const promote: ActionName[] = row.origin.kind === "inferred" && row.rule ? ["promote"] : [];
  return [...actions, ...promote, "disable"];
}

interface RowProps {
  row: ReferenceRow;
  rowKey: string;
  coverage: CoverageState | undefined;
  onCheck(): void;
  ctx: ProjectContext;
}

function RowActions({ row, rowKey, onCheck, ctx }: Omit<RowProps, "coverage">) {
  const send = async (change: ProjectChange, context: string) => {
    const failed = await ctx.change(change);
    if (failed) fail(failed, context);
  };
  const run = (action: ActionName, id: string) => {
    if (action === "check") return onCheck();
    if (action === "edit") {
      const form = referenceFormFromRow(row, `edit ${id}`);
      if (form) ctx.setForm(form);
      return;
    }
    if (action === "remove") return void send({ op: "removeReference", id }, "Could not remove the reference");
    if (action === "enable") return void send({ op: "enable", id }, "Could not enable the reference");
    if (action === "promote" && row.rule) return void send({ op: "addReference", reference: row.rule }, "Could not promote the reference");
    if (action === "disable") return void send({ op: "disable", id }, "Could not disable the reference");
  };
  const id = row.id ?? "";
  return (
    <span className="acts">
      {rowActions(row).map((action, i) => (
        <span key={action}>
          {i > 0 && <span className="o3"> · </span>}
          <button className="act" data-stage={`reference:${rowKey}:${action}`} onClick={() => run(action, id)}>
            {action}
          </button>
        </span>
      ))}
    </span>
  );
}

function ReferenceLine({ row, rowKey, coverage, onCheck, ctx }: RowProps) {
  const inferred = row.origin.kind === "inferred";
  return (
    <div className={rowClass(row)} role="row" aria-label={`${columnsText(row.from)}${row.when ? ` ${row.when}` : ""} → ${columnsText(row.to)}`} data-stage={`reference:${rowKey}`} data-stage-label={columnsText(row.from)} data-stage-list="references">
      <span className="src">{columnsText(row.from)}</span>
      <span className="src">{row.when}</span>
      <span className={`src${inferred ? " dashed" : ""}`}>
        {columnsText(row.to)}
        {row.toWhen && <span className="o3"> where {row.toWhen}</span>}
      </span>
      <OriginCell row={row} />
      <span className="r">{row.reference ? <CoverageCell state={coverage} /> : <span className="o3">not applied</span>}</span>
      <RowActions row={row} rowKey={rowKey} onCheck={onCheck} ctx={ctx} />
    </div>
  );
}

function summary(ctx: ProjectContext): string {
  const { foreignKeys, references } = ctx.rules;
  const keys = foreignKeys === 0 ? "schema foreign keys: none in this database" : `${foreignKeys} schema foreign keys`;
  return `${keys} · ${references.length} rules`;
}

export function ReferencesSection({ ctx }: { ctx: ProjectContext }) {
  const [coverage, setCoverage] = useState<Record<string, CoverageState>>({});
  const { tab, rules } = ctx;

  const check = async (row: ReferenceRow) => {
    const reference = row.reference;
    if (!reference) return;
    const key = referenceKey(row);
    setCoverage((c) => ({ ...c, [key]: { status: "running" } }));
    try {
      const result = await api.referenceCoverage(tab.sessionId, tab.database, reference);
      setCoverage((c) => ({ ...c, [key]: { status: "done", coverage: result } }));
    } catch (e) {
      setCoverage((c) => ({ ...c, [key]: { status: "error", message: toAppError(e).message } }));
    }
  };

  const checkAll = async () => {
    for (const row of rules.references) await check(row);
  };

  return (
    <section aria-label="References">
      <div className="sh">
        <span className="tn sans">references</span>
        <span className="via">{summary(ctx)}</span>
        <span className="spacer" />
        <button className="act" data-stage="project:check-all" onClick={() => void checkAll()}>
          Check coverage for all
        </button>
      </div>
      <div className="row refs hdr" role="row">
        <span>source</span>
        <span>when</span>
        <span>target</span>
        <span>origin</span>
        <span className="r">coverage</span>
        <span />
      </div>
      {rules.references.length === 0 && <div className="row empty o3">no references</div>}
      <LongList items={rules.references}>
        {(row) => {
          const rowKey = referenceKey(row);
          return <ReferenceLine key={rowKey} row={row} rowKey={rowKey} coverage={coverage[rowKey]} onCheck={() => void check(row)} ctx={ctx} />;
        }}
      </LongList>
    </section>
  );
}
