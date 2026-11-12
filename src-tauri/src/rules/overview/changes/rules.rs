//! Change lines for value styles, kinds and table icons.

use super::text::{kind_text, style_text};
use super::{ChangeAction, ChangedRule, EditRule, ProjectChangeLine};
use crate::rules::icons::{self, IconRule};
use crate::rules::kinds::StyleRule;
use crate::rules::overview::{Overview, preset_origin};
use crate::rules::project_change::ProjectChange;

impl Overview<'_> {
    fn style_table(&self, rule: &StyleRule) -> Option<String> {
        let covered = self.covered_tables(&rule.columns, &rule.meta_keys, &rule.r#in);
        covered.into_iter().next()
    }

    pub(super) fn style_changes(&self) -> Vec<ProjectChangeLine> {
        let project = self.project;
        let mut lines = Vec::new();
        for own in &project.styles {
            let shipped = self.applied_presets().find_map(|preset| {
                let rule = preset.styles.iter().find(|s| s.id == own.id)?;
                Some((preset, rule))
            });
            let action = if shipped.is_some() {
                ChangeAction::Edited
            } else {
                ChangeAction::Added
            };
            let undo = ProjectChange::RemoveStyle { id: own.id.clone() };
            lines.push(ProjectChangeLine {
                origin: shipped.map(|(preset, _)| preset_origin(preset)),
                id: Some(own.id.clone()),
                was: shipped.map(|(_, rule)| style_text(rule)),
                off: shipped.is_none() && !own.enabled,
                table: self.style_table(own),
                edit: Some(EditRule::Style { id: own.id.clone() }),
                ..ProjectChangeLine::new(action, ChangedRule::ValueStyle, style_text(own), undo)
            });
        }
        for preset in self.applied_presets() {
            for shipped in &preset.styles {
                let id = shipped.id.clone();
                let (action, undo) = if shipped.enabled && project.disabled.contains(&id) {
                    (ChangeAction::SwitchedOff, ProjectChange::EnableStyle { id })
                } else if !shipped.enabled && project.enabled.contains(&id) {
                    (ChangeAction::SwitchedOn, ProjectChange::DisableStyle { id })
                } else {
                    continue;
                };
                let rule = project.style_override(&shipped.id).unwrap_or(shipped);
                lines.push(ProjectChangeLine {
                    origin: Some(preset_origin(preset)),
                    id: Some(shipped.id.clone()),
                    table: self.style_table(rule),
                    ..ProjectChangeLine::new(
                        action,
                        ChangedRule::ValueStyle,
                        style_text(rule),
                        undo,
                    )
                });
            }
        }
        lines
    }

    pub(super) fn kind_changes(&self) -> Vec<ProjectChangeLine> {
        self.project
            .kinds
            .iter()
            .enumerate()
            .map(|(index, rule)| {
                let covered = self.covered_tables(&rule.columns, &rule.meta_keys, &rule.in_meta);
                let undo = ProjectChange::RemoveKind { index };
                ProjectChangeLine {
                    table: covered.into_iter().next(),
                    edit: Some(EditRule::Kind { index }),
                    ..ProjectChangeLine::new(
                        ChangeAction::Added,
                        ChangedRule::Kind,
                        kind_text(rule),
                        undo,
                    )
                }
            })
            .collect()
    }

    /// The real tables an icon rule names: by name, or every table its glob covers.
    fn icon_tables(&self, rule: &IconRule) -> Vec<String> {
        let applied = &self.rules.applied;
        self.schema
            .tables
            .iter()
            .filter(|t| {
                icons::rule_for(std::slice::from_ref(rule), &applied.rule_names(&t.name)).is_some()
            })
            .map(|t| t.name.clone())
            .collect()
    }

    /// Each project icon: edited where a preset gives the table an icon, else added.
    pub(super) fn icon_changes(&self) -> Vec<ProjectChangeLine> {
        self.project
            .icons
            .iter()
            .map(|own| {
                let table = self.icon_tables(own).into_iter().next();
                let names = table
                    .as_deref()
                    .map(|t| self.rules.applied.rule_names(t))
                    .unwrap_or_default();
                let shipped = icons::preset_rule_for(&self.rules.applied.presets, &names);
                let action = if shipped.is_some() {
                    ChangeAction::Edited
                } else {
                    ChangeAction::Added
                };
                let shown = match &table {
                    Some(real) if !own.is_glob() => real.clone(),
                    _ => own.table.clone(),
                };
                let undo = ProjectChange::RemoveIcon {
                    table: own.table.clone(),
                };
                ProjectChangeLine {
                    origin: shipped.map(|(preset, _)| preset_origin(preset)),
                    id: Some(own.table.clone()),
                    was: shipped.map(|(_, rule)| format!("{} · {}", rule.table, rule.icon.name())),
                    table,
                    ..ProjectChangeLine::new(
                        action,
                        ChangedRule::Icon,
                        format!("{shown} · {}", own.icon.name()),
                        undo,
                    )
                }
            })
            .collect()
    }
}
