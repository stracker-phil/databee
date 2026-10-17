//! The lens seam: what a column holds and which references a table has. `SchemaLens` answers
//! from the schema cache (primary keys, declared foreign keys, same-name inference);
//! `rules::lens::RulesLens` wraps it with the effective rules of presets and the project.

mod annotate;
mod cells;
mod schema_lens;

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use annotate::{annotate_browse, annotate_execute, annotate_table};
pub use schema_lens::SchemaLens;

use crate::db::{ColumnMeta, ResultSet};
use crate::error::AppResult;
use crate::rules::effective::{self, EffectiveRules, RuleSources};
use crate::rules::lens::RulesLens;
use crate::schema::{self, DatabaseSchema};
use crate::session::Session;
use crate::value::{CellAnnotations, ColumnKind, ReferenceSource};

pub trait Lens: Send + Sync {
    /// The kind of each of `columns` (columns of `table`), in order.
    fn kinds(&self, table: &str, columns: &[ColumnMeta]) -> Vec<ColumnKind>;
    /// References whose source columns are in `table`.
    fn references_from(&self, table: &str) -> Vec<ResolvedReference>;
    /// References that point at `table`.
    fn references_to(&self, table: &str) -> Vec<ResolvedReference>;
    /// Kinds and styles that each row decides (conditional references, meta-key kinds, value
    /// styles) for the result columns of one table, after `kinds` filled `result.kinds`.
    fn cells(&self, _columns: &TableColumns<'_>, _result: &ResultSet) -> CellAnnotations {
        CellAnnotations::default()
    }
}

/// The result columns that come from one table: `(position in the result, name in the table)`.
pub struct TableColumns<'a> {
    pub table: &'a str,
    pub columns: Vec<(usize, &'a str)>,
}

/// Columns of one table; a reference's two ends pair up by position.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct ColumnsRef {
    pub table: String,
    pub columns: Vec<String>,
}

impl ColumnsRef {
    pub fn new(table: &str, columns: Vec<String>) -> Self {
        Self {
            table: table.to_string(),
            columns,
        }
    }
}

/// A reference from `from` to `to` and why it exists. An ambiguous inferred column gives one
/// reference per target table.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct ResolvedReference {
    pub from: ColumnsRef,
    pub to: ColumnsRef,
    pub by: ReferenceSource,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct TableReferences {
    pub outgoing: Vec<ResolvedReference>,
    pub incoming: Vec<ResolvedReference>,
}

/// The rules lens when rules change anything, else the schema lens alone.
pub fn build(
    schema: Arc<DatabaseSchema>,
    connection: &str,
    database: &str,
    rules: Option<Arc<EffectiveRules>>,
) -> Box<dyn Lens> {
    let schema_lens = SchemaLens::new(schema, connection, database);
    match rules {
        Some(rules) if !rules.is_empty() => Box::new(RulesLens::new(schema_lens, rules)),
        _ => Box::new(schema_lens),
    }
}

/// The lens of `database` as the session sees it now; without `rules`, the schema's alone.
pub async fn for_database(
    session: &Session,
    database: &str,
    rules: Option<RuleSources<'_>>,
) -> AppResult<Box<dyn Lens>> {
    let schema = schema::database(session, database).await?;
    let effective = rules.map(|sources| effective::effective(session, database, &schema, sources));
    Ok(build(schema, &session.slug(), database, effective))
}

pub async fn table_references(
    session: &Session,
    database: &str,
    table: &str,
    rules: Option<RuleSources<'_>>,
) -> AppResult<TableReferences> {
    let lens = for_database(session, database, rules).await?;
    Ok(TableReferences {
        outgoing: lens.references_from(table),
        incoming: lens.references_to(table),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{connect_to, servers, spec};

    async fn fixture_sessions(database: &str) -> Vec<(String, Session)> {
        let mut out = Vec::new();
        for (host, port) in servers() {
            let db = connect_to(&host, port, database).await;
            let session = Session::new(
                "it".into(),
                spec(&host, port, database),
                format!("it-{port}"),
                Box::new(db),
            );
            out.push((format!("{host}:{port}"), session));
        }
        out
    }

    fn declared_books_author() -> ResolvedReference {
        ResolvedReference {
            from: ColumnsRef::new("books", vec!["author_id".into()]),
            to: ColumnsRef::new("authors", vec!["id".into()]),
            by: ReferenceSource::ForeignKey {
                constraint: "books_author".into(),
            },
        }
    }

    fn same_name_from(refs: &[ResolvedReference], from: ColumnsRef, to: ColumnsRef) -> bool {
        refs.iter().any(|r| {
            r.from == from
                && r.to == to
                && matches!(&r.by, ReferenceSource::SameName { column, .. } if Some(column) == from.columns.first())
        })
    }

    #[tokio::test]
    #[ignore]
    async fn declared_foreign_key_shows_outgoing_from_books_and_incoming_to_authors() {
        for (server, session) in fixture_sessions("databee_test").await {
            let books = table_references(&session, "databee_test", "books", None)
                .await
                .unwrap_or_else(|e| panic!("{server}: {e:?}"));
            let authors = table_references(&session, "databee_test", "authors", None)
                .await
                .unwrap();

            assert_eq!(books.outgoing, [declared_books_author()], "{server}");
            assert_eq!(authors.incoming, [declared_books_author()], "{server}");
        }
    }

    #[tokio::test]
    #[ignore]
    async fn same_name_references_show_on_both_ends_on_the_wp_fixture() {
        for (server, session) in fixture_sessions("wp_test").await {
            let logs = table_references(&session, "wp_test", "wp_actionscheduler_logs", None)
                .await
                .unwrap_or_else(|e| panic!("{server}: {e:?}"));
            let terms = table_references(&session, "wp_test", "wp_terms", None)
                .await
                .unwrap();

            assert!(
                same_name_from(
                    &logs.outgoing,
                    ColumnsRef::new("wp_actionscheduler_logs", vec!["action_id".into()]),
                    ColumnsRef::new("wp_actionscheduler_actions", vec!["action_id".into()]),
                ),
                "{server}: {:?}",
                logs.outgoing
            );
            assert!(
                same_name_from(
                    &terms.incoming,
                    ColumnsRef::new("wp_term_taxonomy", vec!["term_id".into()]),
                    ColumnsRef::new("wp_terms", vec!["term_id".into()]),
                ),
                "{server}: {:?}",
                terms.incoming
            );
        }
    }
}
