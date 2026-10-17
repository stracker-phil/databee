import type { AppError, ProjectChange, ProjectRules } from "../../api";
import type { ProjectForm } from "../../lib/ruleForms";
import type { ProjectTab } from "../../state/store";

/** What every section of the Project page works with. */
export interface ProjectContext {
  tab: ProjectTab;
  rules: ProjectRules;
  /** Stores a change; the error is returned for the caller to show. */
  change(change: ProjectChange): Promise<AppError | null>;
  setForm(form: ProjectForm | null): void;
}
