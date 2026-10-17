//! What a table tab's Structure view shows: one row per column with the kinds, references and
//! value styles that apply to it, the meta-key rules of its meta tables, then the references to
//! it, its entities and its actions. Built from the Project page's rows, so switching, editing
//! and resetting work the same in both places.

use serde::Serialize;
use ts_rs::TS;

use crate::db::{ColumnInfo, IndexInfo, TableStructure};
use crate::error::AppResult;
use crate::rules::actions::{ActionScope, ActionWrite, writes};
use crate::rules::effective::{self, EffectiveRules, MetaRef, RuleSources};
use crate::rules::overview::{self, KindRow, ProjectRules, ReferenceRow, StyleRow, TableRules};
use crate::rules::types::ColumnGlob;
use crate::schema;
use crate::session::Session;

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct TableProfile {
    pub table: String,
    /// The name rules write the table as (without prefix); `None` outside the prefix.
    pub rule_name: Option<String>,
    pub columns: Vec<ColumnProfile>,
    /// The table's own meta-key rules when it is a meta table, then those of the meta tables
    /// that describe its rows.
    pub meta: Vec<MetaKeyRules>,
    /// References from other tables to this one.
    pub referenced_by: Vec<ReferenceRow>,
    pub entities: Vec<EntityProfile>,
    pub actions: Vec<ActionProfile>,
    pub ddl: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ColumnProfile {
    pub column: ColumnInfo,
    pub primary_key: bool,
    pub indexes: Vec<ColumnIndex>,
    /// In precedence order: the first one applies.
    pub kinds: Vec<ColumnKindRule>,
    /// References from this column (the first column of a composite key), switched off ones too.
    pub references: Vec<ReferenceRow>,
    pub styles: Vec<ColumnStyleRule>,
}

/// An index that holds the column; `position` counts from 1.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ColumnIndex {
    pub name: String,
    pub unique: bool,
    pub position: usize,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ColumnKindRule {
    pub row: KindRow,
    /// The glob of the rule that matched the column, when it is one (`*.*_gmt`).
    pub pattern: Option<String>,
    /// The first rule for the column, the one values get.
    pub applies: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ColumnStyleRule {
    pub row: StyleRow,
    pub pattern: Option<String>,
}

/// The rules of one meta table that name meta keys: kinds and value styles by `meta_keys`, and
/// the references from its value column under a condition (on the key, as presets write them).
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct MetaKeyRules {
    pub table: String,
    pub key_column: String,
    pub value_column: String,
    pub kinds: Vec<KindRow>,
    pub styles: Vec<StyleRow>,
    pub references: Vec<ReferenceRow>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct EntityProfile {
    pub preset: String,
    pub name: String,
    pub filter: Option<String>,
    pub summary: Vec<String>,
    pub created: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ActionProfile {
    pub preset: String,
    pub name: String,
    pub scope: ActionScope,
    pub writes: Vec<ActionWrite>,
}

pub async fn for_table(
    session: &Session,
    database: &str,
    table: &str,
    sources: RuleSources<'_>,
) -> AppResult<TableProfile> {
    let (schema, rules) = effective::load(session, database, sources).await?;
    let structure = schema::structure(session, database, table).await?;
    let project = sources.projects.get(&session.slug(), database);
    let page = overview::project_rules(project, schema, rules.clone(), &sources.presets.presets());
    Ok(table_profile(&page, &rules, &structure, table))
}

pub fn table_profile(
    page: &ProjectRules,
    rules: &EffectiveRules,
    structure: &TableStructure,
    table: &str,
) -> TableProfile {
    let empty = TableRules::default();
    let group = group(page, table).unwrap_or(&empty);
    let own_meta = rules.meta_table(table);
    let in_own_meta = |row: &ReferenceRow| own_meta.is_some_and(|m| is_meta_reference(row, m));
    let columns = structure
        .columns
        .iter()
        .map(|column| {
            let name = column.name.as_str();
            let references = group
                .references
                .iter()
                .filter(|r| r.from.columns.first().is_some_and(|c| c == name) && !in_own_meta(r))
                .cloned()
                .collect();
            ColumnProfile {
                column: column.clone(),
                primary_key: structure.primary_key.iter().any(|c| c == name),
                indexes: column_indexes(&structure.indexes, name),
                kinds: column_kinds(rules, &group.kinds, table, name),
                references,
                styles: column_styles(rules, &group.styles, table, name),
            }
        })
        .collect();
    let meta = own_meta
        .into_iter()
        .chain(rules.owned_meta(table).filter(|m| m.table != table))
        .map(|m| meta_key_rules(page, m))
        .collect();
    let referenced_by = page
        .groups
        .iter()
        .filter(|g| g.table.as_deref() != Some(table))
        .flat_map(|g| &g.references)
        .filter(|r| r.to.table == table)
        .cloned()
        .collect();
    let entities = rules
        .entities_of(table)
        .map(|applied| {
            let entity = &applied.entity;
            EntityProfile {
                preset: applied.preset.clone(),
                name: entity.name.clone(),
                filter: entity.filter.clone(),
                summary: entity.summary.clone(),
                created: entity.created.clone(),
            }
        })
        .collect();
    let actions = rules
        .actions
        .iter()
        .filter(|a| a.rule.table == table)
        .map(|a| ActionProfile {
            preset: a.preset.clone(),
            name: a.rule.name.clone(),
            scope: a.rule.scope,
            writes: writes(&a.rule, &rules.applied),
        })
        .collect();
    TableProfile {
        table: table.to_string(),
        rule_name: rules
            .applied
            .rule_names(table)
            .first()
            .map(ToString::to_string),
        columns,
        meta,
        referenced_by,
        entities,
        actions,
        ddl: structure.ddl.clone(),
    }
}

fn group<'p>(page: &'p ProjectRules, table: &str) -> Option<&'p TableRules> {
    page.groups
        .iter()
        .find(|g| g.table.as_deref() == Some(table))
}

fn column_indexes(indexes: &[IndexInfo], column: &str) -> Vec<ColumnIndex> {
    indexes
        .iter()
        .filter_map(|index| {
            let at = index.columns.iter().position(|c| c == column)?;
            Some(ColumnIndex {
                name: index.name.clone(),
                unique: index.unique,
                position: at + 1,
            })
        })
        .collect()
}

/// The glob of `globs` that covers `table.column`: `Some(None)` when it names the column
/// exactly, `Some(Some(glob))` for a pattern.
fn matching_glob(
    rules: &EffectiveRules,
    globs: &[ColumnGlob],
    table: &str,
    column: &str,
) -> Option<Option<String>> {
    let glob = globs
        .iter()
        .find(|g| rules.columns_match(std::slice::from_ref(g), table, column))?;
    let text = glob.to_string();
    Some(text.contains('*').then_some(text))
}

fn column_kinds(
    rules: &EffectiveRules,
    kinds: &[KindRow],
    table: &str,
    column: &str,
) -> Vec<ColumnKindRule> {
    kinds
        .iter()
        .filter_map(|row| {
            let pattern = matching_glob(rules, &row.rule.columns, table, column)?;
            Some((row, pattern))
        })
        .enumerate()
        .map(|(i, (row, pattern))| ColumnKindRule {
            row: row.clone(),
            pattern,
            applies: i == 0,
        })
        .collect()
}

fn column_styles(
    rules: &EffectiveRules,
    styles: &[StyleRow],
    table: &str,
    column: &str,
) -> Vec<ColumnStyleRule> {
    styles
        .iter()
        .filter_map(|row| {
            let pattern = matching_glob(rules, &row.rule.columns, table, column)?;
            Some(ColumnStyleRule {
                row: row.clone(),
                pattern,
            })
        })
        .collect()
}

fn names_meta(meta_keys: &[String], in_meta: &[String], meta: &MetaRef) -> bool {
    !meta_keys.is_empty() && (in_meta.is_empty() || in_meta.contains(&meta.name))
}

fn is_meta_reference(row: &ReferenceRow, meta: &MetaRef) -> bool {
    row.from.table == meta.table && row.from.columns == [meta.value.clone()] && !row.when.is_empty()
}

fn meta_key_rules(page: &ProjectRules, meta: &MetaRef) -> MetaKeyRules {
    let empty = TableRules::default();
    let group = group(page, &meta.table).unwrap_or(&empty);
    MetaKeyRules {
        table: meta.table.clone(),
        key_column: meta.key.clone(),
        value_column: meta.value.clone(),
        kinds: group
            .kinds
            .iter()
            .filter(|k| names_meta(&k.rule.meta_keys, &k.rule.in_meta, meta))
            .cloned()
            .collect(),
        styles: group
            .styles
            .iter()
            .filter(|s| names_meta(&s.rule.meta_keys, &s.rule.r#in, meta))
            .cloned()
            .collect(),
        references: group
            .references
            .iter()
            .filter(|r| is_meta_reference(r, meta))
            .cloned()
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::db::ForeignKey;
    use crate::lens::ColumnsRef;
    use crate::rules::overview::{RuleOrigin, project_rules};
    use crate::rules::project::Project;
    use crate::rules::testing::{preset, project, schema, with_foreign_key};
    use crate::schema::DatabaseSchema;

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
when = { origin = "person" }
to = "people.person_id"

[[reference]]
from = "notes.v"
when = { k = "_ref" }
to = "people.person_id"

[[kind]]
columns = ["*.*_gmt"]
timestamp = "utc"

[[kind]]
columns = ["threads.created_gmt"]
timestamp = "unix"

[[kind]]
meta_keys = ["api_*"]
secret = true

[[style]]
id = "pending"
columns = ["threads.status"]
match = "="
value = "pending"
color = "yellow"

[[style]]
id = "secret_red"
meta_keys = ["api_*"]
match = "LIKE"
value = "x%"
color = "red"

[[entity]]
name = "thread"
table = "threads"
filter = "status = 'open'"
summary = ["status", "author"]
created = "created_gmt"

[[action]]
name = "close"
table = "threads"
update = [
  { set = { status = "closed" } },
  { table = "notes", key = "thread_id", set = { v = "" } },
  { table = "missing", set = { x = "1" } },
]

[[action]]
name = "purge"
table = "threads"
scope = "table"
delete = { where = "status = 'spam'" }
"#;

    const THREADS: &[&str] = &["thread_id", "author", "origin", "status", "created_gmt"];

    fn kit() -> RuleOrigin {
        RuleOrigin::Preset {
            preset: "kit".into(),
        }
    }

    fn db_schema() -> DatabaseSchema {
        let tables = schema(&[
            ("f_threads", THREADS, &["thread_id"]),
            ("f_people", &["person_id"], &["person_id"]),
            ("f_notes", &["note_id", "thread_id", "k", "v"], &["note_id"]),
            ("f_replies", &["reply_id", "thread_id"], &["reply_id"]),
        ]);
        let replies_thread = ForeignKey {
            name: "replies_thread".into(),
            columns: vec!["thread_id".into()],
            ref_table: "f_threads".into(),
            ref_columns: vec!["thread_id".into()],
        };
        with_foreign_key(tables, "f_replies", replies_thread)
    }

    fn structure(columns: &[&str], indexes: Vec<IndexInfo>) -> TableStructure {
        TableStructure {
            columns: columns
                .iter()
                .map(|name| ColumnInfo {
                    name: name.to_string(),
                    column_type: "bigint".into(),
                    nullable: false,
                    default: None,
                    key: String::new(),
                    extra: String::new(),
                    comment: String::new(),
                    auto_increment: false,
                    generated: false,
                })
                .collect(),
            indexes,
            primary_key: vec![columns[0].to_string()],
            foreign_keys: Vec::new(),
            ddl: None,
        }
    }

    fn profile_of(table: &str, columns: &[&str]) -> TableProfile {
        profile_with(table, columns, Vec::new())
    }

    fn profile_with(table: &str, columns: &[&str], indexes: Vec<IndexInfo>) -> TableProfile {
        let own = Project {
            prefix: Some("f_".into()),
            ..project()
        };
        let loaded = [preset(KIT)];
        let schema = Arc::new(db_schema());
        let rules = Arc::new(EffectiveRules::build(&own, &schema, &loaded));
        let page = project_rules(own, schema, rules.clone(), &loaded);
        table_profile(&page, &rules, &structure(columns, indexes), table)
    }

    fn threads() -> TableProfile {
        profile_of("f_threads", THREADS)
    }

    fn column<'p>(profile: &'p TableProfile, name: &str) -> &'p ColumnProfile {
        profile
            .columns
            .iter()
            .find(|c| c.column.name == name)
            .unwrap_or_else(|| panic!("no column {name}"))
    }

    fn cols(table: &str, column: &str) -> ColumnsRef {
        ColumnsRef::new(table, vec![column.to_string()])
    }

    #[test]
    fn a_glob_kind_lists_on_the_columns_it_covers_with_its_pattern() {
        let profile = threads();

        let kinds: Vec<_> = column(&profile, "created_gmt")
            .kinds
            .iter()
            .map(|k| (k.row.origin.clone(), k.pattern.clone(), k.applies))
            .collect();
        assert_eq!(
            kinds,
            [
                (kit(), Some("*.*_gmt".to_string()), true),
                (kit(), None, false),
            ]
        );
        assert!(column(&profile, "status").kinds.is_empty());
    }

    #[test]
    fn a_conditional_reference_lists_on_its_source_column_with_its_condition() {
        let profile = threads();

        let author = column(&profile, "author");
        assert_eq!(author.references.len(), 1);
        let reference = &author.references[0];
        assert_eq!(reference.when, "origin = 'person'");
        assert_eq!(reference.to, cols("f_people", "person_id"));
        assert_eq!(reference.origin, kit());
        assert!(column(&profile, "origin").references.is_empty());
    }

    #[test]
    fn a_style_lists_on_its_column_without_a_pattern() {
        let profile = threads();

        let styles: Vec<_> = column(&profile, "status")
            .styles
            .iter()
            .map(|s| (s.row.id.as_str(), s.pattern.clone()))
            .collect();
        assert_eq!(styles, [("pending", None)]);
    }

    #[test]
    fn index_membership_and_the_primary_key_mark_each_column() {
        let index = IndexInfo {
            name: "by_status".into(),
            unique: false,
            columns: vec!["origin".into(), "status".into()],
        };
        let profile = profile_with("f_threads", THREADS, vec![index]);

        assert!(column(&profile, "thread_id").primary_key);
        assert!(!column(&profile, "status").primary_key);
        assert_eq!(
            column(&profile, "status").indexes,
            [ColumnIndex {
                name: "by_status".into(),
                unique: false,
                position: 2
            }]
        );
        assert!(column(&profile, "author").indexes.is_empty());
    }

    #[test]
    fn a_meta_table_lists_its_meta_key_rules_in_a_block_not_on_its_value_column() {
        let profile = profile_of("f_notes", &["note_id", "thread_id", "k", "v"]);

        assert_eq!(profile.meta.len(), 1);
        let block = &profile.meta[0];
        assert_eq!(
            (
                block.table.as_str(),
                block.key_column.as_str(),
                block.value_column.as_str()
            ),
            ("f_notes", "k", "v")
        );
        assert_eq!(block.kinds.len(), 1);
        assert_eq!(block.kinds[0].rule.meta_keys, ["api_*"]);
        let styles: Vec<&str> = block.styles.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(styles, ["secret_red"]);
        let references: Vec<&str> = block.references.iter().map(|r| r.when.as_str()).collect();
        assert_eq!(references, ["k = '_ref'"]);
        assert!(column(&profile, "v").references.is_empty());
    }

    #[test]
    fn the_owner_of_a_meta_table_lists_that_tables_meta_key_rules() {
        let profile = threads();

        let tables: Vec<&str> = profile.meta.iter().map(|m| m.table.as_str()).collect();
        assert_eq!(tables, ["f_notes"]);
        assert_eq!(profile.meta[0].references.len(), 1);
    }

    #[test]
    fn references_from_other_tables_are_listed_as_referenced_by_with_their_origin() {
        let profile = threads();

        let incoming: Vec<_> = profile
            .referenced_by
            .iter()
            .map(|r| (r.from.clone(), r.origin.clone()))
            .collect();
        assert!(
            incoming.contains(&(
                cols("f_replies", "thread_id"),
                RuleOrigin::ForeignKey {
                    constraint: "replies_thread".into()
                }
            )),
            "{incoming:?}"
        );
        assert!(incoming.iter().all(|(from, _)| from.table != "f_threads"));
    }

    #[test]
    fn entities_list_their_filter_summary_and_created_column() {
        let profile = threads();

        assert_eq!(
            profile.entities,
            [EntityProfile {
                preset: "kit".into(),
                name: "thread".into(),
                filter: Some("status = 'open'".into()),
                summary: vec!["status".into(), "author".into()],
                created: Some("created_gmt".into()),
            }]
        );
    }

    #[test]
    fn actions_list_what_each_step_writes_on_existing_tables() {
        let profile = threads();

        let actions: Vec<_> = profile
            .actions
            .iter()
            .map(|a| (a.name.as_str(), a.scope, a.writes.clone()))
            .collect();
        assert_eq!(
            actions,
            [
                (
                    "close",
                    ActionScope::Row,
                    vec![
                        ActionWrite::Set {
                            table: "f_threads".into(),
                            columns: vec!["status".into()]
                        },
                        ActionWrite::Set {
                            table: "f_notes".into(),
                            columns: vec!["v".into()]
                        },
                    ]
                ),
                (
                    "purge",
                    ActionScope::Table,
                    vec![ActionWrite::DeleteRows {
                        table: "f_threads".into()
                    }]
                ),
            ]
        );
    }

    #[test]
    fn the_rule_name_drops_the_prefix() {
        assert_eq!(threads().rule_name.as_deref(), Some("threads"));
    }
}
