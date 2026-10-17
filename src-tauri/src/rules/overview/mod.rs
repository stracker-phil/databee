//! What the Project page lists: every reference of a database with its origin (project, preset,
//! foreign key, inferred), switched off ones included, plus kinds, value styles, presets and the
//! tables rules can name.

mod rows;

use std::sync::Arc;

pub use rows::{KindRow, PresetRow, ProjectRules, ReferenceRow, RuleOrigin, RuleTable, StyleRow};

use crate::error::AppResult;
use crate::lens::{self, ColumnsRef, Lens, ResolvedReference, SchemaLens};
use crate::rules::condition::conditions_text;
use crate::rules::effective::{self, EffectiveRules, RuleSources, inferred_id};
use crate::rules::project::Project;
use crate::rules::types::{ColumnRef, Conditions, Preset, Reference};
use crate::schema::DatabaseSchema;
use crate::session::Session;
use crate::value::ReferenceSource;

/// The Project page of `database` as the session sees it now.
pub async fn for_database(
    session: &Session,
    database: &str,
    sources: RuleSources<'_>,
) -> AppResult<ProjectRules> {
    let (schema, rules) = effective::load(session, database, sources).await?;
    let project = sources.projects.get(&session.slug(), database);
    Ok(project_rules(
        project,
        schema,
        rules,
        &sources.presets.presets(),
    ))
}

pub fn project_rules(
    project: Project,
    schema: Arc<DatabaseSchema>,
    rules: Arc<EffectiveRules>,
    loaded: &[Arc<Preset>],
) -> ProjectRules {
    let lens = lens::build(
        schema.clone(),
        &project.connection,
        &project.database,
        Some(rules.clone()),
    );
    let active: Vec<ResolvedReference> = schema
        .tables
        .iter()
        .flat_map(|t| lens.references_from(&t.name))
        .collect();
    let overview = Overview {
        project: &project,
        schema: &schema,
        rules: &rules,
        active: &active,
    };
    let mut references = overview.project_references();
    references.extend(overview.preset_references());
    references.extend(overview.schema_references());
    ProjectRules {
        prefix: rules.applied.prefix.clone(),
        suggested_prefix: rules.applied.suggested_prefix.clone(),
        presets: preset_rows(&rules, loaded),
        foreign_keys: active.iter().filter(|r| is_foreign_key(&r.by)).count(),
        references,
        kinds: overview.kinds(),
        styles: overview.styles(),
        tables: overview.rule_tables(),
        project,
    }
}

fn preset_rows(rules: &EffectiveRules, loaded: &[Arc<Preset>]) -> Vec<PresetRow> {
    let applied = rules.applied.preset_names();
    loaded
        .iter()
        .map(|preset| PresetRow {
            name: preset.name.clone(),
            marker_tables: preset
                .marker_tables
                .iter()
                .map(|t| rules.applied.table(t))
                .collect(),
            applied: applied.contains(&preset.name),
        })
        .collect()
}

fn is_foreign_key(by: &ReferenceSource) -> bool {
    matches!(by, ReferenceSource::ForeignKey { .. })
}

struct Overview<'a> {
    project: &'a Project,
    schema: &'a Arc<DatabaseSchema>,
    rules: &'a EffectiveRules,
    active: &'a [ResolvedReference],
}

impl Overview<'_> {
    fn real(&self, column: &ColumnRef) -> Option<ColumnsRef> {
        let table = self.rules.applied.existing_table(&column.table)?;
        Some(ColumnsRef::new(&table, vec![column.column.clone()]))
    }

    /// A row for a stored rule; `None` when its tables do not exist here.
    fn rule_row(
        &self,
        rule: &Reference,
        origin: RuleOrigin,
        by: ReferenceSource,
    ) -> Option<ReferenceRow> {
        let disabled =
            !matches!(origin, RuleOrigin::Project) && self.rules.disabled.contains(&rule.id);
        let resolved = ResolvedReference {
            from: self.real(&rule.from)?,
            to: self.real(&rule.to)?,
            by,
        };
        let reference = self.active.contains(&resolved).then(|| resolved.clone());
        let ResolvedReference { from, to, .. } = resolved;
        Some(ReferenceRow {
            origin,
            id: Some(rule.id.clone()),
            from,
            to,
            when: conditions_text(&rule.when),
            to_when: conditions_text(&rule.to_when),
            label: rule.label.clone(),
            disabled,
            reference,
            rule: Some(rule.clone()),
        })
    }

    fn project_references(&self) -> Vec<ReferenceRow> {
        self.project
            .references
            .iter()
            .filter_map(|rule| {
                let by = ReferenceSource::Project {
                    id: rule.id.clone(),
                };
                self.rule_row(rule, RuleOrigin::Project, by)
            })
            .collect()
    }

    /// In precedence order (the last applied preset first), without rules the project overrides.
    fn preset_references(&self) -> Vec<ReferenceRow> {
        let mut rows = Vec::new();
        for (preset, references) in self.rules.preset_reference_rules(self.project, self.schema) {
            for rule in &references {
                let origin = RuleOrigin::Preset {
                    preset: preset.name.clone(),
                };
                let by = ReferenceSource::Preset {
                    preset: preset.name.clone(),
                    id: rule.id.clone(),
                };
                rows.extend(self.rule_row(rule, origin, by));
            }
        }
        rows
    }

    /// Foreign keys and inferred references, in one pass over what the schema alone gives: an
    /// inferred reference the project switched off is listed as disabled, one a rule replaces is
    /// left out.
    fn schema_references(&self) -> Vec<ReferenceRow> {
        let schema_lens = SchemaLens::new(
            self.schema.clone(),
            &self.project.connection,
            &self.project.database,
        );
        self.schema
            .tables
            .iter()
            .flat_map(|t| schema_lens.references_from(&t.name))
            .filter_map(|reference| match &reference.by {
                ReferenceSource::ForeignKey { constraint } => {
                    let origin = RuleOrigin::ForeignKey {
                        constraint: constraint.clone(),
                    };
                    Some(self.schema_row(reference, origin))
                }
                ReferenceSource::SameName { indexed, .. } => {
                    let origin = RuleOrigin::Inferred { indexed: *indexed };
                    let disabled = inferred_column_id(&reference)
                        .is_some_and(|id| self.rules.disabled.contains(&id));
                    let applies = self.active.contains(&reference);
                    (disabled || applies).then(|| self.schema_row(reference, origin))
                }
                _ => None,
            })
            .collect()
    }

    fn schema_row(&self, reference: ResolvedReference, origin: RuleOrigin) -> ReferenceRow {
        let (id, rule) = match origin {
            RuleOrigin::Inferred { .. } => {
                (inferred_column_id(&reference), self.promoted(&reference))
            }
            _ => (None, None),
        };
        let applies = self.active.contains(&reference);
        ReferenceRow {
            origin,
            id,
            from: reference.from.clone(),
            to: reference.to.clone(),
            when: String::new(),
            to_when: String::new(),
            label: None,
            disabled: !applies,
            reference: applies.then_some(reference),
            rule,
        }
    }

    fn promoted(&self, reference: &ResolvedReference) -> Option<Reference> {
        let ([from], [to]) = (
            reference.from.columns.as_slice(),
            reference.to.columns.as_slice(),
        ) else {
            return None;
        };
        let applied = &self.rules.applied;
        let from_table = *applied.rule_names(&reference.from.table).first()?;
        let to_table = *applied.rule_names(&reference.to.table).first()?;
        Some(Reference {
            id: String::new(),
            from: ColumnRef {
                table: from_table.to_string(),
                column: from.clone(),
            },
            when: Conditions::new(),
            to: ColumnRef {
                table: to_table.to_string(),
                column: to.clone(),
            },
            to_when: Conditions::new(),
            label: None,
            columns: Vec::new(),
        })
    }

    /// Project rules with their index, then the applied presets' rules (last applied first).
    fn listed<R: Clone, T>(
        &self,
        own: &[R],
        of: fn(&Preset) -> &[R],
        row: fn(RuleOrigin, Option<usize>, R) -> T,
    ) -> Vec<T> {
        let own = own
            .iter()
            .enumerate()
            .map(|(i, rule)| row(RuleOrigin::Project, Some(i), rule.clone()));
        let presets = self.rules.applied.presets.iter().rev().flat_map(|preset| {
            let origin = RuleOrigin::Preset {
                preset: preset.name.clone(),
            };
            of(preset)
                .iter()
                .map(move |rule| row(origin.clone(), None, rule.clone()))
        });
        own.chain(presets).collect()
    }

    fn kinds(&self) -> Vec<KindRow> {
        self.listed(
            &self.project.kinds,
            |p| p.kinds.as_slice(),
            |origin, index, rule| KindRow {
                origin,
                index,
                rule,
            },
        )
    }

    fn styles(&self) -> Vec<StyleRow> {
        self.listed(
            &self.project.styles,
            |p| p.styles.as_slice(),
            |origin, index, rule| StyleRow {
                origin,
                index,
                rule,
            },
        )
    }

    fn rule_tables(&self) -> Vec<RuleTable> {
        self.schema
            .tables
            .iter()
            .filter_map(|t| {
                let name = *self.rules.applied.rule_names(&t.name).first()?;
                Some(RuleTable {
                    table: t.name.clone(),
                    name: name.to_string(),
                })
            })
            .collect()
    }
}

fn inferred_column_id(reference: &ResolvedReference) -> Option<String> {
    let [column] = reference.from.columns.as_slice() else {
        return None;
    };
    Some(inferred_id(&reference.from.table, column))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ForeignKey;
    use crate::document::testing::{Rig, shop};
    use crate::rules::testing::{preset, project, reference, schema, with_foreign_key};

    const KIT: &str = r#"
[preset]
name = "kit"
marker_tables = ["threads", "people"]

[[meta]]
table = "notes"
owner = "threads"
fk = "thread_id"
key = "k"
value = "v"

[[reference]]
from = "threads.author"
to = "people.person_id"

[[reference]]
from = "threads.parent_id"
to = "threads.thread_id"

[[reference]]
from = "notes.v"
when = { k = "_ref" }
to = "threads.thread_id"
"#;

    /// One row of a page, reduced to what decides its place: origin, id, disabled and whether
    /// the reference applies.
    type Summary = (RuleOrigin, Option<String>, bool, bool);

    fn kit() -> RuleOrigin {
        RuleOrigin::Preset {
            preset: "kit".into(),
        }
    }

    fn foreign_key() -> RuleOrigin {
        RuleOrigin::ForeignKey {
            constraint: "replies_thread".into(),
        }
    }

    fn inferred() -> RuleOrigin {
        RuleOrigin::Inferred { indexed: false }
    }

    fn id(text: &str) -> Option<String> {
        Some(text.to_string())
    }

    fn cols(table: &str, column: &str) -> ColumnsRef {
        ColumnsRef::new(table, vec![column.to_string()])
    }

    fn db_schema() -> Arc<DatabaseSchema> {
        let tables = schema(&[
            (
                "f_threads",
                &["thread_id", "person_id", "author", "parent_id"],
                &["thread_id"],
            ),
            ("f_people", &["person_id"], &["person_id"]),
            ("f_notes", &["note_id", "thread_id", "k", "v"], &["note_id"]),
            ("f_replies", &["reply_id", "thread_id"], &["reply_id"]),
            ("x_stray", &["stray_id"], &["stray_id"]),
        ]);
        let replies_thread = ForeignKey {
            name: "replies_thread".into(),
            columns: vec!["thread_id".into()],
            ref_table: "f_threads".into(),
            ref_columns: vec!["thread_id".into()],
        };
        Arc::new(with_foreign_key(tables, "f_replies", replies_thread))
    }

    fn page(own: Project) -> ProjectRules {
        let own = Project {
            prefix: Some("f_".into()),
            ..own
        };
        let loaded = [preset(KIT)];
        let schema = db_schema();
        let rules = Arc::new(EffectiveRules::build(&own, &schema, &loaded));
        project_rules(own, schema, rules, &loaded)
    }

    fn summary(page: &ProjectRules) -> Vec<Summary> {
        page.references
            .iter()
            .map(|r| {
                (
                    r.origin.clone(),
                    r.id.clone(),
                    r.disabled,
                    r.reference.is_some(),
                )
            })
            .collect()
    }

    fn row<'p>(page: &'p ProjectRules, id: Option<&str>) -> &'p ReferenceRow {
        page.references
            .iter()
            .find(|r| r.id.as_deref() == id)
            .unwrap_or_else(|| panic!("no row {id:?} in {:?}", summary(page)))
    }

    #[test]
    fn rows_list_preset_rules_then_foreign_keys_then_inferred_references() {
        let page = page(project());

        assert_eq!(
            summary(&page),
            [
                (kit(), id("threads.author"), false, true),
                (kit(), id("threads.parent_id"), false, true),
                (kit(), id("notes.v?k=_ref"), false, true),
                (kit(), id("notes.thread_id"), false, true),
                (inferred(), id("inferred:f_threads.person_id"), false, true),
                (foreign_key(), None, false, true),
            ]
        );
    }

    #[test]
    fn a_preset_rule_row_names_real_tables_and_reads_its_conditions_as_text() {
        let page = page(project());

        let rule = row(&page, Some("notes.v?k=_ref"));

        assert_eq!(rule.from, cols("f_notes", "v"));
        assert_eq!(rule.to, cols("f_threads", "thread_id"));
        assert_eq!(rule.when, "k = '_ref'");
        assert_eq!(rule.to_when, "");
        assert_eq!(
            rule.reference,
            Some(ResolvedReference {
                from: cols("f_notes", "v"),
                to: cols("f_threads", "thread_id"),
                by: ReferenceSource::Preset {
                    preset: "kit".into(),
                    id: "notes.v?k=_ref".into(),
                },
            })
        );
        assert_eq!(
            rule.rule.as_ref().map(|r| r.from.to_string()),
            Some("notes.v".to_string())
        );
    }

    #[test]
    fn a_meta_tables_foreign_key_to_its_owner_is_listed_as_a_preset_rule() {
        let page = page(project());

        let implied = row(&page, Some("notes.thread_id"));

        assert_eq!(implied.origin, kit());
        assert_eq!(implied.from, cols("f_notes", "thread_id"));
        assert_eq!(implied.to, cols("f_threads", "thread_id"));
        assert_eq!(
            implied.rule,
            Some(reference(
                "notes.thread_id",
                "notes.thread_id",
                "threads.thread_id"
            ))
        );
    }

    #[test]
    fn a_foreign_key_row_has_no_id_and_no_rule() {
        let page = page(project());

        assert_eq!(
            row(&page, None),
            &ReferenceRow {
                origin: foreign_key(),
                id: None,
                from: cols("f_replies", "thread_id"),
                to: cols("f_threads", "thread_id"),
                when: String::new(),
                to_when: String::new(),
                label: None,
                disabled: false,
                reference: Some(ResolvedReference {
                    from: cols("f_replies", "thread_id"),
                    to: cols("f_threads", "thread_id"),
                    by: ReferenceSource::ForeignKey {
                        constraint: "replies_thread".into(),
                    },
                }),
                rule: None,
            }
        );
        assert_eq!(page.foreign_keys, 1);
    }

    #[test]
    fn an_inferred_row_has_the_inferred_id_and_the_project_rule_that_promotes_it() {
        let page = page(project());

        assert_eq!(
            row(&page, Some("inferred:f_threads.person_id")),
            &ReferenceRow {
                origin: inferred(),
                id: id("inferred:f_threads.person_id"),
                from: cols("f_threads", "person_id"),
                to: cols("f_people", "person_id"),
                when: String::new(),
                to_when: String::new(),
                label: None,
                disabled: false,
                reference: Some(ResolvedReference {
                    from: cols("f_threads", "person_id"),
                    to: cols("f_people", "person_id"),
                    by: ReferenceSource::SameName {
                        column: "person_id".into(),
                        indexed: false,
                    },
                }),
                rule: Some(reference("", "threads.person_id", "people.person_id")),
            }
        );
    }

    #[test]
    fn a_disabled_preset_rule_stays_listed_as_disabled_without_a_reference() {
        let page = page(Project {
            disabled: vec!["threads.author".into()],
            ..project()
        });

        let author = row(&page, Some("threads.author"));

        assert!(author.disabled);
        assert_eq!(author.reference, None);
        assert_eq!(author.origin, kit());
        assert_eq!(
            summary(&page)
                .into_iter()
                .filter(|(_, _, disabled, applies)| *disabled || !applies)
                .count(),
            1
        );
    }

    #[test]
    fn a_disabled_inferred_reference_stays_listed_as_disabled_without_a_reference() {
        let page = page(Project {
            disabled: vec!["inferred:f_threads.person_id".into()],
            ..project()
        });

        assert_eq!(
            summary(&page),
            [
                (kit(), id("threads.author"), false, true),
                (kit(), id("threads.parent_id"), false, true),
                (kit(), id("notes.v?k=_ref"), false, true),
                (kit(), id("notes.thread_id"), false, true),
                (inferred(), id("inferred:f_threads.person_id"), true, false),
                (foreign_key(), None, false, true),
            ]
        );
        let disabled = row(&page, Some("inferred:f_threads.person_id"));
        assert_eq!(disabled.from, cols("f_threads", "person_id"));
        assert_eq!(disabled.to, cols("f_people", "person_id"));
        assert_eq!(
            disabled.rule,
            Some(reference("", "threads.person_id", "people.person_id"))
        );
    }

    #[test]
    fn a_project_rule_replaces_the_preset_rule_with_the_same_from_and_when() {
        let page = page(Project {
            references: vec![reference("mine", "threads.author", "people.person_id")],
            ..project()
        });

        assert_eq!(
            summary(&page),
            [
                (RuleOrigin::Project, id("mine"), false, true),
                (kit(), id("threads.parent_id"), false, true),
                (kit(), id("notes.v?k=_ref"), false, true),
                (kit(), id("notes.thread_id"), false, true),
                (inferred(), id("inferred:f_threads.person_id"), false, true),
                (foreign_key(), None, false, true),
            ]
        );
        assert_eq!(
            row(&page, Some("mine")).reference,
            Some(ResolvedReference {
                from: cols("f_threads", "author"),
                to: cols("f_people", "person_id"),
                by: ReferenceSource::Project { id: "mine".into() },
            })
        );
    }

    #[test]
    fn tables_hold_only_the_tables_under_the_prefix_by_the_name_rules_use() {
        let page = page(project());

        let tables: Vec<(&str, &str)> = page
            .tables
            .iter()
            .map(|t| (t.table.as_str(), t.name.as_str()))
            .collect();

        assert_eq!(
            tables,
            [
                ("f_threads", "threads"),
                ("f_people", "people"),
                ("f_notes", "notes"),
                ("f_replies", "replies"),
            ]
        );
    }

    #[test]
    fn presets_list_their_real_marker_tables_and_whether_they_apply() {
        let page = page(project());

        assert_eq!(page.prefix, "f_");
        assert_eq!(page.suggested_prefix.as_deref(), Some("f_"));
        assert_eq!(
            page.presets,
            [PresetRow {
                name: "kit".into(),
                marker_tables: vec!["f_threads".into(), "f_people".into()],
                applied: true,
            }]
        );
        assert_eq!(page.project.connection, "local");
    }

    #[tokio::test]
    async fn the_page_of_a_database_reads_its_project_presets_and_rules_from_the_session() {
        let rig = Rig::new(shop(), "");

        let page = for_database(&rig.session, "wp", rig.sources())
            .await
            .unwrap();

        assert_eq!(
            (
                page.project.connection.as_str(),
                page.project.database.as_str()
            ),
            ("local", "wp")
        );
        assert_eq!(page.prefix, "f_");
        assert_eq!(
            page.presets,
            [PresetRow {
                name: "doc".into(),
                marker_tables: vec!["f_orders".into()],
                applied: true,
            }]
        );
        let implied = row(&page, Some("ordermeta.order_id"));
        assert_eq!(
            implied.origin,
            RuleOrigin::Preset {
                preset: "doc".into()
            }
        );
        assert!(implied.reference.is_some());
    }

    #[test]
    fn a_preset_the_project_leaves_out_is_listed_as_not_applied_and_brings_no_rows() {
        let page = page(Project {
            presets: Some(Vec::new()),
            ..project()
        });

        assert!(!page.presets[0].applied);
        assert_eq!(
            summary(&page),
            [
                (inferred(), id("inferred:f_threads.person_id"), false, true),
                (inferred(), id("inferred:f_notes.thread_id"), false, true),
                (foreign_key(), None, false, true),
            ]
        );
    }
}
