import { useEffect, type ReactNode } from "react";
import type { ChangeAction, ChangedRule, ProjectChangeLine, ProjectRules, ReferenceRow } from "../../api";
import { coverageProblem } from "../../lib/coverageProblems";
import { kindFormFromRow, referenceFormFromRow, ruleOriginText, styleFormFromRow, type ProjectForm } from "../../lib/ruleForms";
import { projectKey } from "../../state/slices/project";
import { readoutKey } from "../../state/slices/readout";
import { fail, useStore } from "../../state/store";
import type { ProjectContext } from "./context";
import { coverageKey, useAutoCoverage, type CoverageState } from "./coverage";
import { ActionButtons, referenceLabel } from "./ReferenceLines";

const ACTION_TEXT: Record<ChangeAction, string> = { added: "added", edited: "edited", switchedOff: "switched off", switchedOn: "switched on", set: "set" };

const RULE_TEXT: Record<ChangedRule, string> = { reference: "reference", valueStyle: "value style", kind: "kind", prefix: "prefix", networkPrefix: "network prefix", presets: "presets", unknown: "rule" };

/** What `was` is to the line: the shipped rule, or the value without the project's setting. */
const WAS_TEXT: Record<ChangedRule, string> = { reference: "was", valueStyle: "was", kind: "was", prefix: "suggested", networkPrefix: "suggested", presets: "auto", unknown: "was" };

function undoText(line: ProjectChangeLine): string {
  if (line.action === "added") return "remove";
  if (line.action === "switchedOn") return "switch off";
  return "reset";
}

export const activeReferenceRows = (rules: ProjectRules): ReferenceRow[] => rules.groups.flatMap((g) => g.references.filter((r) => r.reference));

/** The form of the project rule a line edits, from the listed rows. */
function editForm(line: ProjectChangeLine, rules: ProjectRules): ProjectForm | null {
  const edit = line.edit;
  if (!edit) return null;
  const groups = rules.groups;
  if (edit.type === "reference") {
    const row = groups.flatMap((g) => g.references).find((r) => r.id === edit.id);
    return row ? referenceFormFromRow(row, `edit ${edit.id}`) : null;
  }
  if (edit.type === "style") {
    const row = groups.flatMap((g) => g.styles).find((r) => r.id === edit.id && (r.origin.kind === "project" || r.edited));
    return row ? styleFormFromRow(row) : null;
  }
  const row = groups.flatMap((g) => g.kinds).find((r) => r.index === edit.index);
  return row ? kindFormFromRow(row) : null;
}

function openStructure(database: string, table: string) {
  useStore.getState().openTableTab(database, table, "structure");
}

function Band({ name, count, children }: { name: string; count: string; children?: ReactNode }) {
  return (
    <div className="sh">
      <span className="tn sans">{name}</span>
      <span className="cnt">{count}</span>
      <span className="spacer" />
      {children}
    </div>
  );
}

function ChangeLine({ line, ctx }: { line: ProjectChangeLine; ctx: ProjectContext }) {
  const { tab, rules } = ctx;
  const what = `${ACTION_TEXT[line.action]} ${RULE_TEXT[line.rule]}`;
  const label = `${what} ${line.text}`;
  const anchor = `change:${line.action}:${line.rule}:${line.id ?? line.text}`;
  const actions: [string, () => void][] = [];
  const form = editForm(line, rules);
  if (form) actions.push(["edit", () => ctx.setForm(form)]);
  const undo = undoText(line);
  actions.push([
    undo,
    () =>
      void ctx.change(line.undo).then((failed) => {
        if (failed) fail(failed, `Could not ${undo} the ${RULE_TEXT[line.rule]}`);
      }),
  ]);
  const table = line.table;
  return (
    <div className="row change" role="row" aria-label={label} data-stage={anchor} data-stage-label={label} data-stage-list="changes">
      <span className="what">
        {what}
        {line.origin && <span className="o3"> · {ruleOriginText(line.origin)}</span>}
        {line.id && line.rule !== "unknown" && <span className="o3"> · {line.id}</span>}
      </span>
      <span className="text">
        {table ? (
          <button className="link" title={`Open the structure of ${table}`} onClick={() => openStructure(tab.database, table)}>
            {line.text}
          </button>
        ) : (
          line.text
        )}
        {line.off && <span className="o3"> · off</span>}
        {line.was !== null && (
          <span className="o3">
            {" "}
            · {WAS_TEXT[line.rule]} {line.was}
          </span>
        )}
      </span>
      <ActionButtons anchor={anchor} actions={actions} />
    </div>
  );
}

function ChangesSection({ ctx }: { ctx: ProjectContext }) {
  const changes = ctx.rules.changes;
  return (
    <section aria-label="Your changes" className="project-part">
      <Band name="Your changes" count={changes.length ? `${changes.length}` : ""} />
      {changes.length === 0 ? (
        <div className="row empty o3">No changes — this database uses the presets as shipped.</div>
      ) : (
        changes.map((line, i) => <ChangeLine key={`${line.action}:${line.rule}:${line.id ?? line.text}:${i}`} line={line} ctx={ctx} />)
      )}
    </section>
  );
}

interface Problem {
  key: string;
  label: string;
  origin: string;
  text: string;
  table: string | null;
  filter: string | null;
}

function referenceProblem(row: ReferenceRow, state: CoverageState | undefined): Problem | null {
  if (!state || state.status === "running") return null;
  const found = state.status === "error" ? { text: `could not count: ${state.message}`, filter: null } : coverageProblem(state.coverage);
  if (!found) return null;
  const label = referenceLabel(row);
  return { key: `reference:${label}:${row.id ?? ""}`, label, origin: ruleOriginText(row.origin), table: row.from.table, ...found };
}

function useAnomalyProblems(sessionId: string, database: string): Problem[] {
  const entry = useStore((s) => s.readouts[readoutKey(sessionId, database)]);
  const dataVersion = useStore((s) => s.dataVersion);
  useEffect(() => {
    void useStore.getState().loadReadout(sessionId, database);
  }, [sessionId, database, dataVersion]);
  const part = entry?.readout?.anomalies;
  if (!part) return [];
  if (part.error) return [{ key: "anomalies", label: "anomalies", origin: "", text: `could not run: ${part.error}`, table: null, filter: null }];
  return part.items.flatMap((line) => (line.error ? [{ key: `anomaly:${line.preset}:${line.id}`, label: `anomaly ${line.id}`, origin: `preset ${line.preset}`, text: `query failed: ${line.error}`, table: null, filter: null }] : []));
}

function ProblemLine({ problem, database }: { problem: Problem; database: string }) {
  const { table, filter } = problem;
  const anchor = `problem:${problem.key}`;
  const actions: [string, () => void][] = [];
  const open = (filter?: string) => useStore.getState().openTableTab(database, table!, "data", filter);
  if (table && filter) actions.push(["Open rows", () => open(filter)]);
  if (table) actions.push(["Open table", () => open()]);
  return (
    <div className="row problem" role="row" aria-label={problem.label} data-stage={anchor} data-stage-label={problem.label} data-stage-list="problems">
      <span className="text">
        {problem.label}: {problem.text}
        {problem.origin && <span className="o3"> · {problem.origin}</span>}
      </span>
      <ActionButtons anchor={anchor} actions={actions} />
    </div>
  );
}

function ProblemsSection({ ctx }: { ctx: ProjectContext }) {
  const { tab, rules } = ctx;
  const rows = activeReferenceRows(rules);
  const coverage = useAutoCoverage(tab.sessionId, tab.database, rows.map((r) => r.reference!));
  const anomalies = useAnomalyProblems(tab.sessionId, tab.database);
  const states = rows.map((row) => coverage[coverageKey(row.reference!)]);
  const counted = states.filter((s) => s && s.status !== "running").length;
  const problems = [...rows.flatMap((row, i) => referenceProblem(row, states[i]) ?? []), ...anomalies];
  const counting = counted < rows.length;
  const refresh = () => useStore.getState().refreshCoverage(projectKey(tab.sessionId, tab.database));
  return (
    <section aria-label="Problems" className="project-part">
      <Band name="Problems" count={counting ? `counting ${counted} of ${rows.length} references…` : problems.length ? `${problems.length}` : ""}>
        <button className="act" data-stage="project:refresh-coverage" data-stage-label="Refresh coverage" onClick={refresh}>
          Refresh coverage
        </button>
      </Band>
      {problems.length === 0 && !counting && <div className="row empty o3">No problems found.</div>}
      {problems.map((problem) => (
        <ProblemLine key={problem.key} problem={problem} database={tab.database} />
      ))}
    </section>
  );
}

/** Changes to the presets and problems, then a way to every rule. */
export function Overview({ ctx, onAllRules }: { ctx: ProjectContext; onAllRules: () => void }) {
  return (
    <>
      <ChangesSection ctx={ctx} />
      <ProblemsSection ctx={ctx} />
      <div className="project-more">
        <button className="act" data-stage="project:all-rules" data-stage-label="All rules" onClick={onAllRules}>
          All rules…
        </button>
      </div>
    </>
  );
}
