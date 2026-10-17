import { useEffect } from "react";
import type { ProjectChange } from "../../api";
import { useStore, type ProjectTab } from "../../state/store";
import { projectKey } from "../../state/slices/project";
import { ErrorPanel } from "../ErrorPanel";
import { KindForm, StyleForm } from "./KindStyleForms";
import { PresetsSection } from "./PresetsSection";
import { ReferenceForm } from "./ReferenceForm";
import { ReferencesSection } from "./ReferencesSection";
import { KindsSection, StylesSection } from "./RuleSections";
import type { ProjectContext } from "./context";

function useProjectRules(sessionId: string, database: string) {
  const entry = useStore((s) => s.projectRules[projectKey(sessionId, database)]);
  const dataVersion = useStore((s) => s.dataVersion);
  // `dataVersion` is only a refetch trigger: the effect reads nothing from it.
  useEffect(() => {
    void useStore.getState().loadProjectRules(sessionId, database);
  }, [sessionId, database, dataVersion]);
  return entry;
}

function OpenForm({ tab, ctx }: { tab: ProjectTab; ctx: ProjectContext }) {
  const form = tab.form;
  if (!form) return null;
  if (form.type === "reference") return <ReferenceForm key={`reference:${form.opener}`} form={form} ctx={ctx} />;
  if (form.type === "kind") return <KindForm key={`kind:${form.index}:${form.opener}`} form={form} ctx={ctx} />;
  return <StyleForm key={`style:${form.index}:${form.opener}`} form={form} ctx={ctx} />;
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
  return (
    <div className="project" data-stage="project" data-stage-label={`project · ${tab.database}`}>
      <PresetsSection ctx={ctx} />
      <OpenForm tab={tab} ctx={ctx} />
      <ReferencesSection ctx={ctx} />
      <StylesSection ctx={ctx} />
      <KindsSection ctx={ctx} />
    </div>
  );
}
