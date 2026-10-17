import { useEffect, useState } from "react";
import type { ProjectChange } from "../../api";
import { useStore, type ProjectTab } from "../../state/store";
import { projectKey } from "../../state/slices/project";
import type { ProjectSection } from "../../state/slices/tabs";
import { ErrorPanel } from "../ErrorPanel";
import { useCollapsedGroups } from "../object/useCollapsedGroups";
import { KindForm, StyleForm } from "./KindStyleForms";
import { ProjectHeader } from "./ProjectHeader";
import { activeReferenceRows, Overview } from "./Overview";
import { ReferenceForm } from "./ReferenceForm";
import { groupName, TableSection } from "./TableSection";
import type { ProjectContext } from "./context";
import { useAutoCoverage } from "./coverage";

function useProjectRules(sessionId: string, database: string) {
  const entry = useStore((s) => s.projectRules[projectKey(sessionId, database)]);
  const dataVersion = useStore((s) => s.dataVersion);
  // `dataVersion` is only a refetch trigger: the effect reads nothing from it.
  useEffect(() => {
    void useStore.getState().loadProjectRules(sessionId, database);
  }, [sessionId, database, dataVersion]);
  return entry;
}

/** The open form as a modal sheet; every opener (a row's "edit", "r" on a value) sets the tab's form. */
function FormSheet({ tab, ctx }: { tab: ProjectTab; ctx: ProjectContext }) {
  const form = tab.form;
  if (!form) return null;
  if (form.type === "reference") return <ReferenceForm key={`reference:${form.opener}`} form={form} ctx={ctx} />;
  if (form.type === "kind") return <KindForm key={`kind:${form.index}:${form.opener}`} form={form} ctx={ctx} />;
  return <StyleForm key={`style:${form.replacing}:${form.opener}`} form={form} ctx={ctx} />;
}

/** Every rule by table: the listing behind "All rules…". */
function Tables({ ctx }: { ctx: ProjectContext }) {
  const { tab, rules } = ctx;
  const [filter, setFilter] = useState("");
  const coverage = useAutoCoverage(tab.sessionId, tab.database, activeReferenceRows(rules).map((r) => r.reference!));
  const collapse = useCollapsedGroups(tab.address);
  const needle = filter.trim().toLowerCase();
  const refresh = () => useStore.getState().refreshCoverage(projectKey(tab.sessionId, tab.database));
  const shown = needle ? rules.groups.filter((g) => groupName(g).toLowerCase().includes(needle)) : rules.groups;
  return (
    <>
      <div className="project-filter">
        <input className="input mono" type="search" aria-label="Filter tables" data-stage="project:filter" data-stage-label="Filter tables" placeholder="filter tables" value={filter} onChange={(e) => setFilter(e.target.value)} />
        <span className="o3">{needle ? `${shown.length} of ${rules.groups.length} tables` : `${rules.groups.length} tables with rules`}</span>
        <span className="spacer" />
        <button className="act" data-stage="project:refresh-coverage" data-stage-label="Refresh coverage" onClick={refresh}>
          Refresh coverage
        </button>
      </div>
      {shown.length === 0 && <div className="row empty o3">{rules.groups.length === 0 ? "no rules: no table has a reference, value style, kind, entity or action" : "no table matches"}</div>}
      {shown.map((group) => (
        <TableSection key={groupName(group)} group={group} coverage={coverage} collapse={collapse} ctx={ctx} />
      ))}
    </>
  );
}

export function ProjectPanel({ tab }: { tab: ProjectTab }) {
  const entry = useProjectRules(tab.sessionId, tab.database);
  const rules = entry?.rules;
  if (entry?.error && !rules) return <ErrorPanel message={entry.error.message} code={entry.error.code} />;
  if (!rules) return <div className="empty-state">Loading…</div>;
  const ctx: ProjectContext = {
    tab,
    rules,
    change: (change: ProjectChange) => useStore.getState().changeProject(tab.sessionId, tab.database, change),
    setForm: (form) => useStore.getState().setProjectForm(tab.id, form),
  };
  const section = tab.section ?? "overview";
  const show = (next: ProjectSection) => useStore.getState().setProjectSection(tab.id, next);
  return (
    <div className="project" data-stage="project" data-stage-label={`project · ${tab.database}`}>
      <ProjectHeader ctx={ctx}>
        <div className="segmented" role="group" aria-label="Project section" data-stage="project:section" data-stage-label="Overview / All rules">
          <button data-stage="project:overview" aria-pressed={section === "overview"} onClick={() => show("overview")}>
            Overview
          </button>
          <button data-stage="project:rules" aria-pressed={section === "rules"} onClick={() => show("rules")}>
            All rules
          </button>
        </div>
      </ProjectHeader>
      {section === "overview" ? <Overview ctx={ctx} onAllRules={() => show("rules")} /> : <Tables ctx={ctx} />}
      <FormSheet tab={tab} ctx={ctx} />
    </div>
  );
}
