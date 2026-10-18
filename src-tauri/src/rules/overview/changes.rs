//! "Your changes" on the Project page: every way the project deviates from the presets as they
//! ship for this database, one line each with the change that undoes it.

use std::sync::Arc;

use serde::Serialize;
use ts_rs::TS;

use super::rows::{ReferenceRow, RuleOrigin};
use super::{Overview, preset_origin};
use crate::lens::{ColumnsRef, ResolvedReference};
use crate::rules::applied::Applied;
use crate::rules::condition::conditions_text;
use crate::rules::effective::overrides;
use crate::rules::icons::{self, IconRule};
use crate::rules::kinds::{KindRule, StyleMatch, StyleRule, ValueRule};
use crate::rules::project::Project;
use crate::rules::project_change::ProjectChange;
use crate::rules::types::{Preset, Reference};

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub enum ChangeAction {
    Added,
    Edited,
    SwitchedOff,
    SwitchedOn,
    Set,
}

/// `Unknown`: a switched-off id that names no rule of this database.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub enum ChangedRule {
    Reference,
    ValueStyle,
    Kind,
    Icon,
    Prefix,
    NetworkPrefix,
    Presets,
    Unknown,
}

/// The project rule a line opens in its form.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EditRule {
    Reference { id: String },
    Style { id: String },
    Kind { index: usize },
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ProjectChangeLine {
    pub action: ChangeAction,
    pub rule: ChangedRule,
    /// The preset or inferred rule the change is about; `None` for added rules and settings.
    pub origin: Option<RuleOrigin>,
    pub id: Option<String>,
    /// The rule or setting as it applies now, with real table names where the tables exist.
    pub text: String,
    /// What the preset ships, or what the setting would be without the project's value.
    pub was: Option<String>,
    /// An added rule stored switched off.
    pub off: bool,
    /// The real table the rule is about (for a value style or kind, the first its scope covers).
    pub table: Option<String>,
    pub undo: ProjectChange,
    pub edit: Option<EditRule>,
}

impl ProjectChangeLine {
    fn new(action: ChangeAction, rule: ChangedRule, text: String, undo: ProjectChange) -> Self {
        Self {
            action,
            rule,
            origin: None,
            id: None,
            text,
            was: None,
            off: false,
            table: None,
            undo,
            edit: None,
        }
    }
}

fn columns_text(columns: &ColumnsRef) -> String {
    format!("{}.{}", columns.table, columns.columns.join(", "))
}

fn reference_text(from: &ColumnsRef, when: &str, to: &ColumnsRef, to_when: &str) -> String {
    let mut text = columns_text(from);
    if !when.is_empty() {
        text.push_str(&format!(" when {when}"));
    }
    text.push_str(&format!(" → {}", columns_text(to)));
    if !to_when.is_empty() {
        text.push_str(&format!(" where {to_when}"));
    }
    text
}

fn row_text(row: &ReferenceRow) -> String {
    reference_text(&row.from, &row.when, &row.to, &row.to_when)
}

/// `wc_orders.status, meta keys _wc_* in wc_orders_meta`, as the rule stores it.
fn scope_text<T: ToString>(columns: &[T], meta_keys: &[String], in_meta: &[String]) -> String {
    let mut parts = Vec::new();
    if !columns.is_empty() {
        let columns: Vec<String> = columns.iter().map(ToString::to_string).collect();
        parts.push(columns.join(", "));
    }
    if !meta_keys.is_empty() {
        let mut keys = format!("meta keys {}", meta_keys.join(", "));
        if !in_meta.is_empty() {
            keys.push_str(&format!(" in {}", in_meta.join(", ")));
        }
        parts.push(keys);
    }
    parts.join(" · ")
}

fn style_text(rule: &StyleRule) -> String {
    let matching = match rule.matching {
        StyleMatch::Eq => "=",
        StyleMatch::Like => "LIKE",
        StyleMatch::Regexp => "REGEXP",
    };
    let scope = scope_text(&rule.columns, &rule.meta_keys, &rule.r#in);
    format!(
        "{scope} {matching} '{}' · {}",
        rule.value,
        rule.color.name()
    )
}

fn kind_text(rule: &KindRule) -> String {
    let kind = match &rule.value {
        ValueRule::Money(money) if money.currency.is_empty() => "money".to_string(),
        ValueRule::Money(money) => format!("money · currency from {}", money.currency.join(", ")),
        ValueRule::Timestamp(zone) => {
            let zone = serde_json::to_value(zone).ok();
            let zone = zone.as_ref().and_then(|z| z.as_str()).unwrap_or_default();
            format!("timestamp · {zone}")
        }
        ValueRule::Enum => "enum".into(),
        ValueRule::Secret => "secret".into(),
        ValueRule::Number => "number".into(),
    };
    let scope = scope_text(&rule.columns, &rule.meta_keys, &rule.in_meta);
    format!("{scope} · {kind}")
}

fn list_text(names: &[String]) -> String {
    if names.is_empty() {
        return "none".into();
    }
    names.join(", ")
}

impl Overview<'_> {
    /// Settings, then references, value styles, kinds and switched-off ids that name nothing here.
    pub(super) fn changes(
        &self,
        references: &[ReferenceRow],
        loaded: &[Arc<Preset>],
    ) -> Vec<ProjectChangeLine> {
        let mut lines = self.setting_changes(loaded);
        lines.extend(self.reference_changes(references));
        lines.extend(self.style_changes());
        lines.extend(self.kind_changes());
        lines.extend(self.icon_changes());
        lines.extend(self.unknown_switches(references));
        lines
    }

    fn setting_changes(&self, loaded: &[Arc<Preset>]) -> Vec<ProjectChangeLine> {
        let project = self.project;
        let without = |project: Project| Applied::new(&project, self.schema, loaded);
        let mut lines = Vec::new();
        if let Some(chosen) = &project.presets {
            let auto = without(Project {
                presets: None,
                ..project.clone()
            });
            let undo = ProjectChange::SetPresets { presets: None };
            lines.push(ProjectChangeLine {
                was: Some(list_text(&auto.preset_names())),
                ..ProjectChangeLine::new(
                    ChangeAction::Set,
                    ChangedRule::Presets,
                    list_text(chosen),
                    undo,
                )
            });
        }
        if let Some(prefix) = &project.prefix {
            let undo = ProjectChange::SetPrefix { prefix: None };
            lines.push(ProjectChangeLine {
                was: self.rules.applied.suggested_prefix.clone(),
                ..ProjectChangeLine::new(
                    ChangeAction::Set,
                    ChangedRule::Prefix,
                    prefix.clone(),
                    undo,
                )
            });
        }
        if let Some(prefix) = &project.network_prefix {
            let suggested = without(Project {
                network_prefix: None,
                ..project.clone()
            });
            let undo = ProjectChange::SetNetworkPrefix { prefix: None };
            lines.push(ProjectChangeLine {
                was: Some(suggested.network_prefix),
                ..ProjectChangeLine::new(
                    ChangeAction::Set,
                    ChangedRule::NetworkPrefix,
                    prefix.clone(),
                    undo,
                )
            });
        }
        lines
    }

    fn reference_changes(&self, rows: &[ReferenceRow]) -> Vec<ProjectChangeLine> {
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

    fn style_table(&self, rule: &StyleRule) -> Option<String> {
        let covered = self.covered_tables(&rule.columns, &rule.meta_keys, &rule.r#in);
        covered.into_iter().next()
    }

    fn style_changes(&self) -> Vec<ProjectChangeLine> {
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

    fn kind_changes(&self) -> Vec<ProjectChangeLine> {
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
    fn icon_changes(&self) -> Vec<ProjectChangeLine> {
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

    /// Switched-off ids that no listed reference or applied preset style carries.
    fn unknown_switches(&self, rows: &[ReferenceRow]) -> Vec<ProjectChangeLine> {
        let known = |id: &String| {
            rows.iter().any(|r| r.id.as_ref() == Some(id))
                || self.project.references.iter().any(|r| &r.id == id)
                || self.project.styles.iter().any(|s| &s.id == id)
                || self
                    .applied_presets()
                    .any(|p| p.styles.iter().any(|s| &s.id == id))
        };
        self.project
            .disabled
            .iter()
            .filter(|id| !known(id))
            .map(|id| ProjectChangeLine {
                id: Some(id.clone()),
                ..ProjectChangeLine::new(
                    ChangeAction::SwitchedOff,
                    ChangedRule::Unknown,
                    id.clone(),
                    ProjectChange::Enable { id: id.clone() },
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::project_rules;
    use super::*;
    use crate::rules::effective::EffectiveRules;
    use crate::rules::icons::TableIcon;
    use crate::rules::kinds::StyleColor;
    use crate::rules::testing::{preset, project, reference, schema};
    use crate::rules::types::{ColumnGlob, Conditions, Match};
    use crate::schema::DatabaseSchema;

    const KIT: &str = r#"
[preset]
name = "kit"
marker_tables = ["threads", "people"]

[[reference]]
from = "threads.author"
to = "people.person_id"

[[reference]]
from = "notes.v"
when = { k = "_ref" }
to = "threads.thread_id"

[[style]]
columns = ["threads.author"]
match = "="
value = "x"
color = "red"
enabled = false

[[style]]
id = "roots"
columns = ["threads.parent_id"]
match = "="
value = "0"
color = "green"

[[icon]]
table = "threads"
icon = "list"
"#;

    const AUTHOR_X: &str = "style:threads.author=x";

    fn kit() -> Option<RuleOrigin> {
        Some(RuleOrigin::Preset {
            preset: "kit".into(),
        })
    }

    fn inferred() -> Option<RuleOrigin> {
        Some(RuleOrigin::Inferred { indexed: false })
    }

    fn db_schema() -> Arc<DatabaseSchema> {
        Arc::new(schema(&[
            (
                "f_threads",
                &["thread_id", "person_id", "author", "parent_id"],
                &["thread_id"],
            ),
            ("f_people", &["person_id"], &["person_id"]),
            ("f_notes", &["note_id", "thread_id", "k", "v"], &["note_id"]),
            (
                "f_replies",
                &["reply_id", "thread_id", "author"],
                &["reply_id"],
            ),
        ]))
    }

    fn changes(own: Project) -> Vec<ProjectChangeLine> {
        let loaded = [preset(KIT)];
        let schema = db_schema();
        let rules = Arc::new(EffectiveRules::build(&own, &schema, &loaded));
        project_rules(own, schema, rules, &loaded).changes
    }

    fn line(
        action: ChangeAction,
        rule: ChangedRule,
        text: &str,
        undo: ProjectChange,
    ) -> ProjectChangeLine {
        ProjectChangeLine::new(action, rule, text.into(), undo)
    }

    fn enable(id: &str) -> ProjectChange {
        ProjectChange::Enable { id: id.into() }
    }

    fn remove_reference(id: &str) -> ProjectChange {
        ProjectChange::RemoveReference { id: id.into() }
    }

    fn remove_style(id: &str) -> ProjectChange {
        ProjectChange::RemoveStyle { id: id.into() }
    }

    fn edit_reference(id: &str) -> Option<EditRule> {
        Some(EditRule::Reference { id: id.into() })
    }

    fn edit_style(id: &str) -> Option<EditRule> {
        Some(EditRule::Style { id: id.into() })
    }

    fn some(text: &str) -> Option<String> {
        Some(text.into())
    }

    fn globs(columns: &[&str]) -> Vec<ColumnGlob> {
        columns
            .iter()
            .map(|c| ColumnGlob::try_from(c.to_string()).unwrap())
            .collect()
    }

    fn style_on(id: &str, columns: &[&str], color: StyleColor) -> StyleRule {
        StyleRule {
            id: id.into(),
            columns: globs(columns),
            meta_keys: Vec::new(),
            r#in: Vec::new(),
            matching: StyleMatch::Eq,
            value: "x".into(),
            color,
            enabled: true,
        }
    }

    fn number_kind(columns: &[&str]) -> KindRule {
        KindRule {
            columns: globs(columns),
            meta_keys: Vec::new(),
            in_meta: Vec::new(),
            value: ValueRule::Number,
        }
    }

    #[test]
    fn a_project_without_deviations_has_no_changes() {
        assert_eq!(changes(project()), Vec::<ProjectChangeLine>::new());
    }

    #[test]
    fn choosing_presets_explicitly_is_a_setting_that_remembers_what_auto_would_apply() {
        for (chosen, text) in [(vec!["kit".to_string()], "kit"), (Vec::new(), "none")] {
            let lines = changes(Project {
                presets: Some(chosen.clone()),
                ..project()
            });

            assert_eq!(
                lines,
                [ProjectChangeLine {
                    was: some("kit"),
                    ..line(
                        ChangeAction::Set,
                        ChangedRule::Presets,
                        text,
                        ProjectChange::SetPresets { presets: None },
                    )
                }],
                "{chosen:?}"
            );
        }
    }

    #[test]
    fn a_prefix_is_a_setting_that_remembers_the_suggested_prefix() {
        let lines = changes(Project {
            prefix: some("x_"),
            ..project()
        });

        assert_eq!(
            lines,
            [ProjectChangeLine {
                was: some("f_"),
                ..line(
                    ChangeAction::Set,
                    ChangedRule::Prefix,
                    "x_",
                    ProjectChange::SetPrefix { prefix: None },
                )
            }]
        );
    }

    #[test]
    fn a_network_prefix_is_a_setting_that_remembers_the_prefix_it_replaces() {
        let lines = changes(Project {
            network_prefix: some("n_"),
            ..project()
        });

        assert_eq!(
            lines,
            [ProjectChangeLine {
                was: some("f_"),
                ..line(
                    ChangeAction::Set,
                    ChangedRule::NetworkPrefix,
                    "n_",
                    ProjectChange::SetNetworkPrefix { prefix: None },
                )
            }]
        );
    }

    #[test]
    fn a_switched_off_preset_reference_is_listed_with_real_table_names_and_enable_as_undo() {
        let lines = changes(Project {
            disabled: vec!["threads.author".into()],
            ..project()
        });

        assert_eq!(
            lines,
            [ProjectChangeLine {
                origin: kit(),
                id: some("threads.author"),
                table: some("f_threads"),
                ..line(
                    ChangeAction::SwitchedOff,
                    ChangedRule::Reference,
                    "f_threads.author → f_people.person_id",
                    enable("threads.author"),
                )
            }]
        );
    }

    #[test]
    fn a_switched_off_inferred_reference_is_listed_with_its_inferred_id() {
        let lines = changes(Project {
            disabled: vec!["inferred:f_threads.person_id".into()],
            ..project()
        });

        assert_eq!(
            lines,
            [ProjectChangeLine {
                origin: inferred(),
                id: some("inferred:f_threads.person_id"),
                table: some("f_threads"),
                ..line(
                    ChangeAction::SwitchedOff,
                    ChangedRule::Reference,
                    "f_threads.person_id → f_people.person_id",
                    enable("inferred:f_threads.person_id"),
                )
            }]
        );
    }

    #[test]
    fn an_edited_preset_reference_shows_the_shipped_rule_as_was() {
        let lines = changes(Project {
            references: vec![reference("threads.author", "threads.author", "people.name")],
            ..project()
        });

        assert_eq!(
            lines,
            [ProjectChangeLine {
                origin: kit(),
                id: some("threads.author"),
                was: some("f_threads.author → f_people.person_id"),
                table: some("f_threads"),
                edit: edit_reference("threads.author"),
                ..line(
                    ChangeAction::Edited,
                    ChangedRule::Reference,
                    "f_threads.author → f_people.name",
                    remove_reference("threads.author"),
                )
            }]
        );
    }

    #[test]
    fn an_edited_and_switched_off_preset_reference_gives_two_lines() {
        let lines = changes(Project {
            references: vec![reference("threads.author", "threads.author", "people.name")],
            disabled: vec!["threads.author".into()],
            ..project()
        });

        assert_eq!(
            lines
                .iter()
                .map(|l| (l.action, l.text.as_str(), l.undo.clone()))
                .collect::<Vec<_>>(),
            [
                (
                    ChangeAction::Edited,
                    "f_threads.author → f_people.name",
                    remove_reference("threads.author"),
                ),
                (
                    ChangeAction::SwitchedOff,
                    "f_threads.author → f_people.name",
                    enable("threads.author"),
                ),
            ]
        );
        assert_eq!(lines[1].edit, None);
        assert_eq!(lines[1].was, None);
    }

    #[test]
    fn an_edited_inferred_reference_shows_the_inference_as_was() {
        let lines = changes(Project {
            references: vec![reference("mine", "threads.person_id", "threads.thread_id")],
            ..project()
        });

        assert_eq!(
            lines,
            [ProjectChangeLine {
                origin: inferred(),
                id: some("mine"),
                was: some("f_threads.person_id → f_people.person_id"),
                table: some("f_threads"),
                edit: edit_reference("mine"),
                ..line(
                    ChangeAction::Edited,
                    ChangedRule::Reference,
                    "f_threads.person_id → f_threads.thread_id",
                    remove_reference("mine"),
                )
            }]
        );
    }

    #[test]
    fn an_added_project_reference_has_no_origin_and_can_be_edited_and_removed() {
        let lines = changes(Project {
            references: vec![reference("mine", "replies.author", "people.person_id")],
            ..project()
        });

        assert_eq!(
            lines,
            [ProjectChangeLine {
                id: some("mine"),
                table: some("f_replies"),
                edit: edit_reference("mine"),
                ..line(
                    ChangeAction::Added,
                    ChangedRule::Reference,
                    "f_replies.author → f_people.person_id",
                    remove_reference("mine"),
                )
            }]
        );
    }

    #[test]
    fn an_added_project_reference_stored_off_is_marked_off() {
        let lines = changes(Project {
            references: vec![reference("mine", "replies.author", "people.person_id")],
            disabled: vec!["mine".into()],
            ..project()
        });

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].action, ChangeAction::Added);
        assert!(lines[0].off);
    }

    #[test]
    fn an_added_project_reference_reads_its_conditions_into_the_text() {
        let lines = changes(Project {
            references: vec![Reference {
                when: Conditions::from([("kind".to_string(), Match::Eq("a".into()))]),
                to_when: Conditions::from([("active".to_string(), Match::Eq("1".into()))]),
                ..reference("mine", "replies.author", "people.person_id")
            }],
            ..project()
        });

        assert_eq!(
            lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
            ["f_replies.author when kind = 'a' → f_people.person_id where active = '1'"]
        );
    }

    #[test]
    fn an_added_project_reference_on_missing_tables_keeps_its_stored_names_and_has_no_table() {
        let lines = changes(Project {
            references: vec![reference("ghost", "ghosts.x", "people.person_id")],
            disabled: vec!["ghost".into()],
            ..project()
        });

        assert_eq!(
            lines,
            [ProjectChangeLine {
                id: some("ghost"),
                off: true,
                ..line(
                    ChangeAction::Added,
                    ChangedRule::Reference,
                    "ghosts.x → people.person_id",
                    remove_reference("ghost"),
                )
            }]
        );
    }

    #[test]
    fn a_preset_style_switched_on_is_undone_by_disabling_it() {
        let lines = changes(Project {
            enabled: vec![AUTHOR_X.into()],
            ..project()
        });

        assert_eq!(
            lines,
            [ProjectChangeLine {
                origin: kit(),
                id: some(AUTHOR_X),
                table: some("f_threads"),
                ..line(
                    ChangeAction::SwitchedOn,
                    ChangedRule::ValueStyle,
                    "threads.author = 'x' · red",
                    ProjectChange::DisableStyle {
                        id: AUTHOR_X.into()
                    },
                )
            }]
        );
    }

    #[test]
    fn a_preset_style_switched_off_is_undone_by_enabling_it() {
        let lines = changes(Project {
            disabled: vec!["roots".into()],
            ..project()
        });

        assert_eq!(
            lines,
            [ProjectChangeLine {
                origin: kit(),
                id: some("roots"),
                table: some("f_threads"),
                ..line(
                    ChangeAction::SwitchedOff,
                    ChangedRule::ValueStyle,
                    "threads.parent_id = '0' · green",
                    ProjectChange::EnableStyle { id: "roots".into() },
                )
            }]
        );
    }

    #[test]
    fn an_edited_preset_style_shows_the_shipped_style_as_was() {
        let lines = changes(Project {
            styles: vec![style_on(AUTHOR_X, &["threads.author"], StyleColor::Blue)],
            ..project()
        });

        assert_eq!(
            lines,
            [ProjectChangeLine {
                origin: kit(),
                id: some(AUTHOR_X),
                was: some("threads.author = 'x' · red"),
                table: some("f_threads"),
                edit: edit_style(AUTHOR_X),
                ..line(
                    ChangeAction::Edited,
                    ChangedRule::ValueStyle,
                    "threads.author = 'x' · blue",
                    remove_style(AUTHOR_X),
                )
            }]
        );
    }

    #[test]
    fn an_added_style_has_no_origin_and_is_marked_off_when_stored_off() {
        let on = style_on("mine", &["threads.author"], StyleColor::Yellow);
        let off = StyleRule {
            enabled: false,
            ..on.clone()
        };

        for (style, is_off) in [(on, false), (off, true)] {
            let lines = changes(Project {
                styles: vec![style],
                ..project()
            });

            assert_eq!(
                lines,
                [ProjectChangeLine {
                    id: some("mine"),
                    off: is_off,
                    table: some("f_threads"),
                    edit: edit_style("mine"),
                    ..line(
                        ChangeAction::Added,
                        ChangedRule::ValueStyle,
                        "threads.author = 'x' · yellow",
                        remove_style("mine"),
                    )
                }],
                "off: {is_off}"
            );
        }
    }

    #[test]
    fn a_style_names_the_first_real_table_its_scope_covers_and_none_when_it_covers_nothing() {
        for (columns, table) in [("*.author", Some("f_threads")), ("*.nothing", None)] {
            let lines = changes(Project {
                styles: vec![style_on("mine", &[columns], StyleColor::Yellow)],
                ..project()
            });

            assert_eq!(
                lines.iter().map(|l| l.table.as_deref()).collect::<Vec<_>>(),
                [table],
                "{columns}"
            );
        }
    }

    #[test]
    fn added_kinds_are_listed_by_their_index_in_the_project() {
        let lines = changes(Project {
            kinds: vec![
                number_kind(&["threads.author"]),
                number_kind(&["people.person_id"]),
            ],
            ..project()
        });

        assert_eq!(
            lines,
            [
                ProjectChangeLine {
                    table: some("f_threads"),
                    edit: Some(EditRule::Kind { index: 0 }),
                    ..line(
                        ChangeAction::Added,
                        ChangedRule::Kind,
                        "threads.author · number",
                        ProjectChange::RemoveKind { index: 0 },
                    )
                },
                ProjectChangeLine {
                    table: some("f_people"),
                    edit: Some(EditRule::Kind { index: 1 }),
                    ..line(
                        ChangeAction::Added,
                        ChangedRule::Kind,
                        "people.person_id · number",
                        ProjectChange::RemoveKind { index: 1 },
                    )
                },
            ]
        );
    }

    fn icon_rule(table: &str, icon: TableIcon) -> IconRule {
        IconRule {
            table: table.into(),
            icon,
        }
    }

    fn remove_icon(table: &str) -> ProjectChange {
        ProjectChange::RemoveIcon {
            table: table.into(),
        }
    }

    #[test]
    fn a_project_icon_over_a_preset_icon_is_an_edit_that_remembers_the_shipped_rule() {
        let lines = changes(Project {
            icons: vec![icon_rule("threads", TableIcon::Star)],
            ..project()
        });

        assert_eq!(
            lines,
            [ProjectChangeLine {
                origin: kit(),
                id: some("threads"),
                was: some("threads · list"),
                table: some("f_threads"),
                ..line(
                    ChangeAction::Edited,
                    ChangedRule::Icon,
                    "f_threads · star",
                    remove_icon("threads"),
                )
            }]
        );
    }

    #[test]
    fn a_project_icon_none_over_a_preset_icon_is_an_edit() {
        let lines = changes(Project {
            icons: vec![icon_rule("threads", TableIcon::None)],
            ..project()
        });

        assert_eq!(
            lines,
            [ProjectChangeLine {
                origin: kit(),
                id: some("threads"),
                was: some("threads · list"),
                table: some("f_threads"),
                ..line(
                    ChangeAction::Edited,
                    ChangedRule::Icon,
                    "f_threads · none",
                    remove_icon("threads"),
                )
            }]
        );
    }

    #[test]
    fn a_project_icon_where_no_preset_gives_one_is_added() {
        let lines = changes(Project {
            icons: vec![icon_rule("replies", TableIcon::MessageSquare)],
            ..project()
        });

        assert_eq!(
            lines,
            [ProjectChangeLine {
                id: some("replies"),
                table: some("f_replies"),
                ..line(
                    ChangeAction::Added,
                    ChangedRule::Icon,
                    "f_replies · message-square",
                    remove_icon("replies"),
                )
            }]
        );
    }

    #[test]
    fn a_glob_icon_shows_its_pattern_and_names_the_first_table_it_covers() {
        let lines = changes(Project {
            icons: vec![icon_rule("*s", TableIcon::Tag)],
            ..project()
        });

        assert_eq!(
            lines,
            [ProjectChangeLine {
                origin: kit(),
                id: some("*s"),
                was: some("threads · list"),
                table: some("f_threads"),
                ..line(
                    ChangeAction::Edited,
                    ChangedRule::Icon,
                    "*s · tag",
                    remove_icon("*s"),
                )
            }]
        );
    }

    #[test]
    fn an_icon_for_a_table_the_database_lacks_is_added_with_its_stored_name_and_no_table() {
        let lines = changes(Project {
            icons: vec![icon_rule("ghosts", TableIcon::Box)],
            ..project()
        });

        assert_eq!(
            lines,
            [ProjectChangeLine {
                id: some("ghosts"),
                ..line(
                    ChangeAction::Added,
                    ChangedRule::Icon,
                    "ghosts · box",
                    remove_icon("ghosts"),
                )
            }]
        );
    }

    #[test]
    fn icon_lines_come_after_kinds_and_before_unknown_ids() {
        let lines = changes(Project {
            disabled: vec!["ghost.id".into()],
            kinds: vec![number_kind(&["threads.author"])],
            icons: vec![icon_rule("replies", TableIcon::Tag)],
            ..project()
        });

        assert_eq!(
            lines.iter().map(|l| l.rule).collect::<Vec<_>>(),
            [ChangedRule::Kind, ChangedRule::Icon, ChangedRule::Unknown]
        );
    }

    #[test]
    fn a_switched_off_id_that_names_nothing_is_an_unknown_rule_that_enable_undoes() {
        let lines = changes(Project {
            disabled: vec!["ghost.id".into()],
            ..project()
        });

        assert_eq!(
            lines,
            [ProjectChangeLine {
                id: some("ghost.id"),
                ..line(
                    ChangeAction::SwitchedOff,
                    ChangedRule::Unknown,
                    "ghost.id",
                    enable("ghost.id"),
                )
            }]
        );
    }

    #[test]
    fn changes_list_settings_first_then_references_styles_kinds_and_unknown_ids() {
        let lines = changes(Project {
            disabled: vec!["ghost.id".into()],
            kinds: vec![number_kind(&["threads.author"])],
            styles: vec![style_on("mine", &["threads.author"], StyleColor::Yellow)],
            references: vec![reference(
                "replies_author",
                "replies.author",
                "people.person_id",
            )],
            network_prefix: some("n_"),
            prefix: some("f_"),
            presets: Some(vec!["kit".into()]),
            ..project()
        });

        assert_eq!(
            lines.iter().map(|l| l.rule).collect::<Vec<_>>(),
            [
                ChangedRule::Presets,
                ChangedRule::Prefix,
                ChangedRule::NetworkPrefix,
                ChangedRule::Reference,
                ChangedRule::ValueStyle,
                ChangedRule::Kind,
                ChangedRule::Unknown,
            ]
        );
    }
}
