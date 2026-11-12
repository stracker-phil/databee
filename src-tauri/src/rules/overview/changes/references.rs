//! Change lines for references: added, edited and switched-off rules, and project rules whose
//! tables this database lacks.

use super::text::{reference_text, row_text};
use super::{ChangeAction, ChangedRule, EditRule, ProjectChangeLine};
use crate::lens::{ColumnsRef, ResolvedReference};
use crate::rules::condition::conditions_text;
use crate::rules::effective::overrides;
use crate::rules::overview::Overview;
use crate::rules::overview::rows::{ReferenceRow, RuleOrigin};
use crate::rules::project_change::ProjectChange;
use crate::rules::types::Reference;

impl Overview<'_> {
    pub(super) fn reference_changes(&self, rows: &[ReferenceRow]) -> Vec<ProjectChangeLine> {
        let mut lines = Vec::new();
        for row in rows {
            let Some(id) = row.id.clone() else { continue };
            let line = |action, undo| ProjectChangeLine {
                id: Some(id.clone()),
                table: Some(row.from.table.clone()),
                ..ProjectChangeLine::new(action, ChangedRule::Reference, row_text(row), undo)
            };
            let remove = ProjectChange::RemoveReference { id: id.clone() };
            let edit = Some(EditRule::Reference { id: id.clone() });
            if row.origin == RuleOrigin::Project {
                lines.push(ProjectChangeLine {
                    off: row.disabled,
                    edit,
                    ..line(ChangeAction::Added, remove)
                });
                continue;
            }
            let origin = Some(row.origin.clone());
            if row.edited {
                lines.push(ProjectChangeLine {
                    origin: origin.clone(),
                    was: row.rule.as_ref().and_then(|own| self.replaced_text(own)),
                    edit,
                    ..line(ChangeAction::Edited, remove)
                });
            }
            if row.disabled {
                let enable = ProjectChange::Enable { id: id.clone() };
                lines.push(ProjectChangeLine {
                    origin,
                    ..line(ChangeAction::SwitchedOff, enable)
                });
            }
        }
        lines.extend(self.unplaced_references(rows));
        lines
    }

    /// Project references whose tables this database lacks, as stored.
    fn unplaced_references<'r>(
        &'r self,
        rows: &'r [ReferenceRow],
    ) -> impl Iterator<Item = ProjectChangeLine> + 'r {
        self.project
            .references
            .iter()
            .filter(|own| !rows.iter().any(|r| r.id.as_deref() == Some(&own.id)))
            .map(|own| {
                let when = conditions_text(&own.when);
                let to_when = conditions_text(&own.to_when);
                let from = ColumnsRef::new(&own.from.table, vec![own.from.column.clone()]);
                let to = ColumnsRef::new(&own.to.table, vec![own.to.column.clone()]);
                let undo = ProjectChange::RemoveReference { id: own.id.clone() };
                let text = reference_text(&from, &when, &to, &to_when);
                ProjectChangeLine {
                    id: Some(own.id.clone()),
                    off: self.project.disabled.contains(&own.id),
                    ..ProjectChangeLine::new(
                        ChangeAction::Added,
                        ChangedRule::Reference,
                        text,
                        undo,
                    )
                }
            })
    }

    /// The preset rule or inference the project rule `own` replaces, as it would apply.
    fn replaced_text(&self, own: &Reference) -> Option<String> {
        let shipped = self
            .rules
            .preset_reference_rules(self.schema)
            .flat_map(|(_, rules)| rules)
            .find(|rule| overrides(own, rule));
        if let Some(rule) = shipped {
            let (from, to) = (self.real(&rule.from)?, self.real(&rule.to)?);
            let (when, to_when) = (conditions_text(&rule.when), conditions_text(&rule.to_when));
            return Some(reference_text(&from, &when, &to, &to_when));
        }
        let from = self.real(&own.from)?;
        self.inferred_references()
            .into_iter()
            .find(|r| r.from == from)
            .map(|ResolvedReference { from, to, .. }| reference_text(&from, "", &to, ""))
    }
}
