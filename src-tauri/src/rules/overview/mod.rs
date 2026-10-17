//! What the Project page lists, grouped by table: every reference of a database with its origin
//! (project, preset, foreign key, inferred), switched off and edited ones included, the value styles
//! and kinds whose scope covers the table, its entities and actions, plus the presets and the
//! tables rules can name.

mod rows;

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

pub use rows::{
    ActionLine, EntityLine, KindRow, PresetRow, ProjectRules, ReferenceRow, ReferencedBy,
    RuleOrigin, RuleTable, StyleRow, TableRules,
};

use crate::error::AppResult;
use crate::lens::{ColumnsRef, Lens, ResolvedReference, SchemaLens};
use crate::rules::condition::conditions_text;
use crate::rules::effective::{self, EffectiveRules, RuleSources, inferred_id, overrides};
use crate::rules::lens;
use crate::rules::project::Project;
use crate::rules::types::{ColumnGlob, ColumnRef, Conditions, Preset, Reference};
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
    ProjectRules {
        prefix: rules.applied.prefix.clone(),
        suggested_prefix: rules.applied.suggested_prefix.clone(),
        presets: preset_rows(&rules, loaded),
        foreign_keys: active.iter().filter(|r| is_foreign_key(&r.by)).count(),
        tables: overview.rule_tables(),
        groups: overview.groups(),
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
    fn groups(&self) -> Vec<TableRules> {
        let mut tables = BTreeMap::<String, TableRules>::new();
        let mut nowhere = TableRules::default();
        for row in self.references() {
            if row.to.table != row.from.table {
                let incoming = ReferencedBy {
                    from: row.from.clone(),
                    to: row.to.clone(),
                    when: row.when.clone(),
                    disabled: row.disabled,
                };
                entry(&mut tables, &row.to.table)
                    .referenced_by
                    .push(incoming);
            }
            entry(&mut tables, &row.from.table).references.push(row);
        }
        for row in self.styles() {
            let rule = &row.rule;
            let covered = self.covered_tables(&rule.columns, &rule.meta_keys, &rule.r#in);
            for table in &covered {
                entry(&mut tables, table).styles.push(row.clone());
            }
            if covered.is_empty() {
                nowhere.styles.push(row);
            }
        }
        for row in self.kinds() {
            let rule = &row.rule;
            let covered = self.covered_tables(&rule.columns, &rule.meta_keys, &rule.in_meta);
            for table in &covered {
                entry(&mut tables, table).kinds.push(row.clone());
            }
            if covered.is_empty() {
                nowhere.kinds.push(row);
            }
        }
        for applied in &self.rules.entities {
            let entity = &applied.entity;
            entry(&mut tables, &entity.table).entities.push(EntityLine {
                preset: applied.preset.clone(),
                name: entity.name.clone(),
                filter: entity.filter.clone(),
            });
        }
        for action in &self.rules.actions {
            entry(&mut tables, &action.rule.table)
                .actions
                .push(ActionLine {
                    preset: action.preset.clone(),
                    name: action.rule.name.clone(),
                    scope: action.rule.scope,
                });
        }
        let mut groups: Vec<TableRules> = tables.into_values().collect();
        for group in &mut groups {
            group
                .references
                .sort_by(|a, b| a.from.columns.cmp(&b.from.columns));
        }
        if !nowhere.styles.is_empty() || !nowhere.kinds.is_empty() {
            groups.push(nowhere);
        }
        groups
    }

    /// The real tables a kind or style scope covers: a column matches one of `columns`, or the
    /// table is a meta table in scope of `meta_keys`.
    fn covered_tables(
        &self,
        columns: &[ColumnGlob],
        meta_keys: &[String],
        in_meta: &[String],
    ) -> Vec<String> {
        let by_column = |table: &str| {
            self.schema
                .columns(table)
                .iter()
                .any(|c| self.rules.columns_match(columns, table, c))
        };
        let by_meta_key = |table: &str| {
            !meta_keys.is_empty()
                && self
                    .rules
                    .meta_table(table)
                    .is_some_and(|m| in_meta.is_empty() || in_meta.contains(&m.name))
        };
        self.schema
            .tables
            .iter()
            .filter(|t| by_column(&t.name) || by_meta_key(&t.name))
            .map(|t| t.name.clone())
            .collect()
    }

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
        edited: bool,
    ) -> Option<ReferenceRow> {
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
            disabled: self.rules.disabled.contains(&rule.id),
            edited,
            reference,
            rule: Some(rule.clone()),
        })
    }

    /// Project rules (each in the row of the preset or inferred reference it replaces), then the
    /// preset rules no project rule replaces (last applied preset first), then foreign keys and
    /// inferred references.
    fn references(&self) -> Vec<ReferenceRow> {
        let presets: Vec<_> = self.rules.preset_reference_rules(self.schema).collect();
        let inferred = self.inferred_references();
        let mut replaced = HashSet::new();
        let mut rows = Vec::new();
        for own in &self.project.references {
            let by = ReferenceSource::Project { id: own.id.clone() };
            let preset = presets
                .iter()
                .find(|(_, rules)| rules.iter().any(|r| overrides(own, r)));
            let origin = match (preset, self.replaced_inference(own, &inferred)) {
                (Some((preset, _)), _) => preset_origin(preset),
                (None, Some((id, indexed))) => {
                    replaced.insert(id);
                    RuleOrigin::Inferred { indexed }
                }
                (None, None) => RuleOrigin::Project,
            };
            let edited = origin != RuleOrigin::Project;
            rows.extend(self.rule_row(own, origin, by, edited));
        }
        for (preset, rules) in &presets {
            let kept = rules
                .iter()
                .filter(|r| !self.project.references.iter().any(|own| overrides(own, r)));
            for rule in kept {
                let by = ReferenceSource::Preset {
                    preset: preset.name.clone(),
                    id: rule.id.clone(),
                };
                rows.extend(self.rule_row(rule, preset_origin(preset), by, false));
            }
        }
        rows.extend(self.schema_references(&replaced));
        rows
    }

    fn schema_lens(&self) -> SchemaLens {
        SchemaLens::new(
            self.schema.clone(),
            &self.project.connection,
            &self.project.database,
        )
    }

    fn inferred_references(&self) -> Vec<ResolvedReference> {
        let schema_lens = self.schema_lens();
        self.schema
            .tables
            .iter()
            .flat_map(|t| schema_lens.references_from(&t.name))
            .filter(|r| matches!(r.by, ReferenceSource::SameName { .. }))
            .collect()
    }

    /// The inferred id and `indexed` of the same-name inference an unconditional project rule
    /// replaces on its column.
    fn replaced_inference(
        &self,
        own: &Reference,
        inferred: &[ResolvedReference],
    ) -> Option<(String, bool)> {
        if !own.when.is_empty() {
            return None;
        }
        let from = self.real(&own.from)?;
        inferred.iter().find(|r| r.from == from).and_then(|r| {
            let ReferenceSource::SameName { indexed, .. } = r.by else {
                return None;
            };
            Some((inferred_column_id(r)?, indexed))
        })
    }

    /// Foreign keys and inferred references, in one pass over what the schema alone gives: an
    /// inferred reference the project switched off is listed as disabled, one a rule replaces is
    /// left out (or listed by the project rule that edits it, `replaced`).
    fn schema_references(&self, replaced: &HashSet<String>) -> Vec<ReferenceRow> {
        let schema_lens = self.schema_lens();
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
                    let id = inferred_column_id(&reference)?;
                    if replaced.contains(&id) {
                        return None;
                    }
                    let disabled = self.rules.disabled.contains(&id);
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
                (inferred_column_id(&reference), self.replacement(&reference))
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
            edited: false,
            reference: applies.then_some(reference),
            rule,
        }
    }

    /// The project rule that would replace the inferred `reference` (an empty id: it gets its
    /// default when stored).
    fn replacement(&self, reference: &ResolvedReference) -> Option<Reference> {
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
    fn kinds(&self) -> Vec<KindRow> {
        let own = self
            .project
            .kinds
            .iter()
            .enumerate()
            .map(|(i, rule)| KindRow {
                origin: RuleOrigin::Project,
                index: Some(i),
                rule: rule.clone(),
            });
        let presets = self.applied_presets().flat_map(|preset| {
            preset.kinds.iter().map(|rule| KindRow {
                origin: preset_origin(preset),
                index: None,
                rule: rule.clone(),
            })
        });
        own.chain(presets).collect()
    }

    /// Project styles, then the applied presets' styles (last applied first), each listed once:
    /// a project override shows in its preset style's row.
    fn styles(&self) -> Vec<StyleRow> {
        let shipped = |id: &str| {
            self.applied_presets()
                .any(|p| p.styles.iter().any(|s| s.id == id))
        };
        let own = self
            .project
            .styles
            .iter()
            .filter(|rule| !shipped(&rule.id))
            .map(|rule| StyleRow {
                origin: RuleOrigin::Project,
                id: rule.id.clone(),
                rule: rule.clone(),
                disabled: !rule.enabled,
                edited: false,
            });
        let presets = self.applied_presets().flat_map(|preset| {
            preset.styles.iter().map(|rule| {
                let edited = self.project.style_override(&rule.id);
                StyleRow {
                    origin: preset_origin(preset),
                    id: rule.id.clone(),
                    rule: edited.unwrap_or(rule).clone(),
                    disabled: !self.project.preset_style_on(rule),
                    edited: edited.is_some(),
                }
            })
        });
        own.chain(presets).collect()
    }

    fn applied_presets(&self) -> impl Iterator<Item = &Arc<Preset>> {
        self.rules.applied.presets.iter().rev()
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

fn entry<'t>(tables: &'t mut BTreeMap<String, TableRules>, table: &str) -> &'t mut TableRules {
    tables
        .entry(table.to_string())
        .or_insert_with(|| TableRules {
            table: Some(table.to_string()),
            ..TableRules::default()
        })
}

fn preset_origin(preset: &Preset) -> RuleOrigin {
    RuleOrigin::Preset {
        preset: preset.name.clone(),
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
    use crate::query::testing::{Rig, shop};
    use crate::rules::actions::ActionScope;
    use crate::rules::kinds::{KindRule, StyleColor, StyleMatch, StyleRule, ValueRule};
    use crate::rules::testing::{preset, project, reference, schema, with_foreign_key};
    use crate::rules::types::{ColumnGlob, Match};

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

[[kind]]
columns = ["*.author"]
enum = true

[[kind]]
meta_keys = ["api_*"]
secret = true

[[entity]]
name = "thread"
table = "threads"
filter = "parent_id = 0"

[[entity]]
name = "person"
table = "people"

[[action]]
name = "purge"
table = "notes"
delete = {}

[[action]]
name = "prune"
table = "notes"
scope = "table"
delete = { where = "k = 'x'" }
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
            (
                "f_replies",
                &["reply_id", "thread_id", "author"],
                &["reply_id"],
            ),
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

    fn group<'p>(page: &'p ProjectRules, table: &str) -> &'p TableRules {
        page.groups
            .iter()
            .find(|g| g.table.as_deref() == Some(table))
            .unwrap_or_else(|| panic!("no group {table} in {:?}", tables_of(page)))
    }

    fn tables_of(page: &ProjectRules) -> Vec<Option<&str>> {
        page.groups.iter().map(|g| g.table.as_deref()).collect()
    }

    fn summarize(group: &TableRules) -> Vec<Summary> {
        group
            .references
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

    fn summary(page: &ProjectRules) -> Vec<Summary> {
        page.groups.iter().flat_map(summarize).collect()
    }

    fn row<'p>(page: &'p ProjectRules, id: Option<&str>) -> &'p ReferenceRow {
        page.groups
            .iter()
            .flat_map(|g| &g.references)
            .find(|r| r.id.as_deref() == id)
            .unwrap_or_else(|| panic!("no row {id:?} in {:?}", summary(page)))
    }

    fn by_project(id: &str) -> ReferenceSource {
        ReferenceSource::Project { id: id.into() }
    }

    fn incoming(from: (&str, &str), to: (&str, &str), when: &str, disabled: bool) -> ReferencedBy {
        ReferencedBy {
            from: cols(from.0, from.1),
            to: cols(to.0, to.1),
            when: when.into(),
            disabled,
        }
    }

    #[test]
    fn groups_are_sorted_by_table_and_list_the_references_from_it() {
        let page = page(project());

        assert_eq!(
            tables_of(&page),
            [
                Some("f_notes"),
                Some("f_people"),
                Some("f_replies"),
                Some("f_threads")
            ]
        );
        assert_eq!(
            summarize(group(&page, "f_notes")),
            [
                (kit(), id("notes.thread_id"), false, true),
                (kit(), id("notes.v?k=_ref"), false, true),
            ]
        );
        assert!(group(&page, "f_people").references.is_empty());
        assert_eq!(
            summarize(group(&page, "f_replies")),
            [(foreign_key(), None, false, true)]
        );
        assert_eq!(
            summarize(group(&page, "f_threads")),
            [
                (kit(), id("threads.author"), false, true),
                (kit(), id("threads.parent_id"), false, true),
                (inferred(), id("inferred:f_threads.person_id"), false, true),
            ]
        );
    }

    #[test]
    fn a_target_lists_the_references_from_other_tables_but_not_its_own() {
        let page = page(project());

        assert_eq!(
            group(&page, "f_people").referenced_by,
            [
                incoming(
                    ("f_threads", "author"),
                    ("f_people", "person_id"),
                    "",
                    false
                ),
                incoming(
                    ("f_threads", "person_id"),
                    ("f_people", "person_id"),
                    "",
                    false
                ),
            ]
        );
        assert_eq!(
            group(&page, "f_threads").referenced_by,
            [
                incoming(
                    ("f_notes", "v"),
                    ("f_threads", "thread_id"),
                    "k = '_ref'",
                    false
                ),
                incoming(
                    ("f_notes", "thread_id"),
                    ("f_threads", "thread_id"),
                    "",
                    false
                ),
                incoming(
                    ("f_replies", "thread_id"),
                    ("f_threads", "thread_id"),
                    "",
                    false
                ),
            ]
        );
        assert!(group(&page, "f_notes").referenced_by.is_empty());
        assert!(group(&page, "f_replies").referenced_by.is_empty());
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
                edited: false,
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
                edited: false,
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
            summarize(group(&page, "f_threads")),
            [
                (kit(), id("threads.author"), false, true),
                (kit(), id("threads.parent_id"), false, true),
                (inferred(), id("inferred:f_threads.person_id"), true, false),
            ]
        );
        assert!(group(&page, "f_people").referenced_by.contains(&incoming(
            ("f_threads", "person_id"),
            ("f_people", "person_id"),
            "",
            true
        )));
        let disabled = row(&page, Some("inferred:f_threads.person_id"));
        assert_eq!(disabled.from, cols("f_threads", "person_id"));
        assert_eq!(disabled.to, cols("f_people", "person_id"));
        assert_eq!(
            disabled.rule,
            Some(reference("", "threads.person_id", "people.person_id"))
        );
    }

    #[test]
    fn a_project_rule_with_a_preset_rules_id_shows_once_as_that_rule_edited() {
        let edited = reference("threads.author", "threads.author", "people.name");
        let page = page(Project {
            references: vec![edited.clone()],
            ..project()
        });

        assert_eq!(
            summarize(group(&page, "f_threads")),
            [
                (kit(), id("threads.author"), false, true),
                (kit(), id("threads.parent_id"), false, true),
                (inferred(), id("inferred:f_threads.person_id"), false, true),
            ]
        );
        assert_eq!(
            row(&page, Some("threads.author")),
            &ReferenceRow {
                origin: kit(),
                id: id("threads.author"),
                from: cols("f_threads", "author"),
                to: cols("f_people", "name"),
                when: String::new(),
                to_when: String::new(),
                label: None,
                disabled: false,
                edited: true,
                reference: Some(ResolvedReference {
                    from: cols("f_threads", "author"),
                    to: cols("f_people", "name"),
                    by: by_project("threads.author"),
                }),
                rule: Some(edited),
            }
        );
        assert!(group(&page, "f_people").referenced_by.contains(&incoming(
            ("f_threads", "author"),
            ("f_people", "name"),
            "",
            false
        )));
    }

    #[test]
    fn a_project_rule_with_a_preset_rules_from_and_when_replaces_it_under_its_own_id() {
        let page = page(Project {
            references: vec![reference("mine", "threads.author", "people.name")],
            ..project()
        });

        assert_eq!(
            summarize(group(&page, "f_threads")),
            [
                (kit(), id("mine"), false, true),
                (kit(), id("threads.parent_id"), false, true),
                (inferred(), id("inferred:f_threads.person_id"), false, true),
            ]
        );
        let edited = row(&page, Some("mine"));
        assert!(edited.edited);
        assert_eq!(edited.to, cols("f_people", "name"));
        assert_eq!(
            edited.reference.as_ref().map(|r| r.by.clone()),
            Some(by_project("mine"))
        );
    }

    #[test]
    fn a_project_rule_that_replaces_nothing_is_a_project_row_and_follows_its_switch() {
        let rule = reference("mine", "replies.author", "people.person_id");
        let on = page(Project {
            references: vec![rule.clone()],
            ..project()
        });
        let off = page(Project {
            references: vec![rule],
            disabled: vec!["mine".into()],
            ..project()
        });

        assert_eq!(
            summarize(group(&on, "f_replies")),
            [
                (RuleOrigin::Project, id("mine"), false, true),
                (foreign_key(), None, false, true),
            ]
        );
        assert!(!row(&on, Some("mine")).edited);
        assert_eq!(
            summarize(group(&off, "f_replies")),
            [
                (RuleOrigin::Project, id("mine"), true, false),
                (foreign_key(), None, false, true),
            ]
        );
        assert!(group(&off, "f_people").referenced_by.contains(&incoming(
            ("f_replies", "author"),
            ("f_people", "person_id"),
            "",
            true
        )));
    }

    #[test]
    fn an_unconditional_project_rule_on_an_inferred_column_shows_as_that_inference_edited() {
        let page = page(Project {
            references: vec![Reference {
                label: Some("owner".into()),
                ..reference("mine", "threads.person_id", "people.person_id")
            }],
            ..project()
        });

        assert_eq!(
            summarize(group(&page, "f_threads")),
            [
                (kit(), id("threads.author"), false, true),
                (kit(), id("threads.parent_id"), false, true),
                (inferred(), id("mine"), false, true),
            ]
        );
        let edited = row(&page, Some("mine"));
        assert!(edited.edited);
        assert_eq!(edited.label.as_deref(), Some("owner"));
        assert!(
            page.groups
                .iter()
                .flat_map(|g| &g.references)
                .all(|r| r.id.as_deref() != Some("inferred:f_threads.person_id"))
        );
    }

    #[test]
    fn switching_an_edited_inferred_reference_off_keeps_one_disabled_row_and_no_inference() {
        let page = page(Project {
            references: vec![reference("mine", "threads.person_id", "people.person_id")],
            disabled: vec!["mine".into()],
            ..project()
        });

        assert_eq!(
            summarize(group(&page, "f_threads")),
            [
                (kit(), id("threads.author"), false, true),
                (kit(), id("threads.parent_id"), false, true),
                (inferred(), id("mine"), true, false),
            ]
        );
        assert!(row(&page, Some("mine")).edited);
        assert_eq!(row(&page, Some("mine")).reference, None);
        assert_eq!(
            group(&page, "f_people").referenced_by,
            [
                incoming(
                    ("f_threads", "person_id"),
                    ("f_people", "person_id"),
                    "",
                    true
                ),
                incoming(
                    ("f_threads", "author"),
                    ("f_people", "person_id"),
                    "",
                    false
                ),
            ]
        );
    }

    #[test]
    fn a_conditional_project_rule_on_an_inferred_column_leaves_the_inference_listed() {
        let conditional = Reference {
            when: Conditions::from([("author".to_string(), Match::Eq("1".into()))]),
            ..reference("mine", "threads.person_id", "people.name")
        };
        let page = page(Project {
            references: vec![conditional],
            ..project()
        });

        assert_eq!(
            summarize(group(&page, "f_threads")),
            [
                (kit(), id("threads.author"), false, true),
                (kit(), id("threads.parent_id"), false, true),
                (RuleOrigin::Project, id("mine"), false, true),
                (inferred(), id("inferred:f_threads.person_id"), false, true),
            ]
        );
    }

    const AUTHOR_X: &str = "style:threads.author=x";

    /// Origin, id, color, disabled, edited.
    fn style_summary(group: &TableRules) -> Vec<(RuleOrigin, &str, &str, bool, bool)> {
        group
            .styles
            .iter()
            .map(|r| {
                let color = r.rule.color.name();
                (r.origin.clone(), r.id.as_str(), color, r.disabled, r.edited)
            })
            .collect()
    }

    fn globs(columns: &[&str]) -> Vec<ColumnGlob> {
        columns
            .iter()
            .map(|c| ColumnGlob::try_from(c.to_string()).unwrap())
            .collect()
    }

    fn style_on(id: &str, columns: &[&str], meta_keys: &[&str], color: StyleColor) -> StyleRule {
        StyleRule {
            id: id.into(),
            columns: globs(columns),
            meta_keys: meta_keys.iter().map(|k| k.to_string()).collect(),
            r#in: Vec::new(),
            matching: StyleMatch::Eq,
            value: "x".into(),
            color,
            enabled: true,
        }
    }

    fn author_style(id: &str, color: StyleColor) -> StyleRule {
        style_on(id, &["threads.author"], &[], color)
    }

    fn number_kind(columns: &[&str]) -> KindRule {
        KindRule {
            columns: globs(columns),
            meta_keys: Vec::new(),
            in_meta: Vec::new(),
            value: ValueRule::Number,
        }
    }

    /// Origin, project index and meta keys of the kinds on a table.
    fn kind_summary(group: &TableRules) -> Vec<(RuleOrigin, Option<usize>, Vec<String>)> {
        group
            .kinds
            .iter()
            .map(|k| (k.origin.clone(), k.index, k.rule.meta_keys.clone()))
            .collect()
    }

    #[test]
    fn preset_styles_are_listed_once_with_their_switch_state() {
        let shipped = page(project());
        let switched = page(Project {
            enabled: vec![AUTHOR_X.into()],
            disabled: vec!["roots".into()],
            ..project()
        });

        assert_eq!(
            style_summary(group(&shipped, "f_threads")),
            [
                (kit(), AUTHOR_X, "red", true, false),
                (kit(), "roots", "green", false, false),
            ]
        );
        assert_eq!(
            style_summary(group(&switched, "f_threads")),
            [
                (kit(), AUTHOR_X, "red", false, false),
                (kit(), "roots", "green", true, false),
            ]
        );
    }

    #[test]
    fn a_project_override_shows_in_its_preset_row_and_own_styles_come_first() {
        let page = page(Project {
            styles: vec![
                author_style(AUTHOR_X, StyleColor::Blue),
                author_style("mine", StyleColor::Yellow),
            ],
            ..project()
        });

        assert_eq!(
            style_summary(group(&page, "f_threads")),
            [
                (RuleOrigin::Project, "mine", "yellow", false, false),
                (kit(), AUTHOR_X, "blue", true, true),
                (kit(), "roots", "green", false, false),
            ]
        );
    }

    #[test]
    fn a_project_style_stored_off_is_listed_as_disabled() {
        let off = StyleRule {
            enabled: false,
            ..author_style("mine", StyleColor::Yellow)
        };
        let page = page(Project {
            styles: vec![off],
            ..project()
        });

        assert_eq!(
            style_summary(group(&page, "f_threads"))[0],
            (RuleOrigin::Project, "mine", "yellow", true, false)
        );
    }

    #[test]
    fn styles_and_kinds_are_listed_on_every_table_their_scope_covers() {
        let page = page(Project {
            styles: vec![
                style_on("wide", &["*.author"], &[], StyleColor::Yellow),
                style_on("flags", &[], &["flag"], StyleColor::Green),
            ],
            kinds: vec![number_kind(&["*.person_id"])],
            ..project()
        });

        assert_eq!(
            style_summary(group(&page, "f_replies")),
            [(RuleOrigin::Project, "wide", "yellow", false, false)]
        );
        assert_eq!(
            style_summary(group(&page, "f_notes")),
            [(RuleOrigin::Project, "flags", "green", false, false)]
        );
        assert_eq!(
            style_summary(group(&page, "f_threads")),
            [
                (RuleOrigin::Project, "wide", "yellow", false, false),
                (kit(), AUTHOR_X, "red", true, false),
                (kit(), "roots", "green", false, false),
            ]
        );
        assert!(group(&page, "f_people").styles.is_empty());
        assert_eq!(
            kind_summary(group(&page, "f_threads")),
            [
                (RuleOrigin::Project, Some(0), Vec::<String>::new()),
                (kit(), None, Vec::<String>::new()),
            ]
        );
        assert_eq!(
            kind_summary(group(&page, "f_people")),
            [(RuleOrigin::Project, Some(0), Vec::<String>::new())]
        );
        assert_eq!(
            kind_summary(group(&page, "f_replies")),
            [(kit(), None, Vec::<String>::new())]
        );
        assert_eq!(
            kind_summary(group(&page, "f_notes")),
            [(kit(), None, vec!["api_*".to_string()])]
        );
    }

    #[test]
    fn styles_and_kinds_covering_no_table_land_in_a_last_group_without_a_table() {
        let page = page(Project {
            styles: vec![style_on("ghost", &["ghosts.x"], &[], StyleColor::Blue)],
            kinds: vec![number_kind(&["ghosts.*"])],
            ..project()
        });

        let last = page.groups.last().unwrap();

        assert_eq!(last.table, None);
        assert_eq!(
            style_summary(last),
            [(RuleOrigin::Project, "ghost", "blue", false, false)]
        );
        assert_eq!(
            kind_summary(last),
            [(RuleOrigin::Project, Some(0), Vec::<String>::new())]
        );
        assert!(last.references.is_empty() && last.referenced_by.is_empty());
        assert_eq!(page.groups.iter().filter(|g| g.table.is_none()).count(), 1);
        assert!(
            page.groups
                .iter()
                .filter(|g| g.table.is_some())
                .all(|g| g.styles.iter().all(|s| s.id != "ghost"))
        );
    }

    #[test]
    fn a_page_with_every_rule_placed_has_no_group_without_a_table() {
        let page = page(project());

        assert!(page.groups.iter().all(|g| g.table.is_some()));
    }

    #[test]
    fn entities_and_actions_of_a_preset_are_listed_on_their_table() {
        let page = page(project());

        assert_eq!(
            group(&page, "f_threads").entities,
            [EntityLine {
                preset: "kit".into(),
                name: "thread".into(),
                filter: Some("parent_id = 0".into()),
            }]
        );
        assert_eq!(
            group(&page, "f_people").entities,
            [EntityLine {
                preset: "kit".into(),
                name: "person".into(),
                filter: None,
            }]
        );
        assert_eq!(
            group(&page, "f_notes").actions,
            [
                ActionLine {
                    preset: "kit".into(),
                    name: "purge".into(),
                    scope: ActionScope::Row,
                },
                ActionLine {
                    preset: "kit".into(),
                    name: "prune".into(),
                    scope: ActionScope::Table,
                },
            ]
        );
        assert!(group(&page, "f_threads").actions.is_empty());
        assert!(group(&page, "f_notes").entities.is_empty());
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
        assert!(
            group(&page, &implied.from.table)
                .references
                .iter()
                .any(|r| r.id.as_deref() == Some("ordermeta.order_id"))
        );
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
                (inferred(), id("inferred:f_notes.thread_id"), false, true),
                (foreign_key(), None, false, true),
                (inferred(), id("inferred:f_threads.person_id"), false, true),
            ]
        );
        for group in &page.groups {
            assert!(
                group.styles.is_empty()
                    && group.kinds.is_empty()
                    && group.entities.is_empty()
                    && group.actions.is_empty(),
                "{:?}",
                group.table
            );
        }
    }
}
