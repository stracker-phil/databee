//! Where a new rule for one value starts: the value's column as rules name it and, on a meta
//! value, the meta key it is stored under.

use std::time::Duration;

use serde::Serialize;
use ts_rs::TS;

use super::effective::{self, RuleSources};
use super::types::ColumnRef;
use crate::error::{AppError, AppResult};
use crate::schema;
use crate::session::Session;
use crate::value::{Provenance, stored_key};

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct MetaDraft {
    pub table: String,
    pub key_column: String,
    pub key: String,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct RuleDraft {
    /// The value's column, its table without prefix (`wc_orders_meta.meta_value`).
    pub source: ColumnRef,
    pub meta: Option<MetaDraft>,
}

pub async fn rule_draft(
    session: &Session,
    database: &str,
    provenance: &Provenance,
    sources: RuleSources<'_>,
    timeout: Duration,
) -> AppResult<RuleDraft> {
    let schema = schema::database(session, database).await?;
    let rules = effective::effective(session, database, &schema, sources);
    let Some(name) = rules.applied.rule_names(&provenance.table).first().copied() else {
        return Err(AppError::InvalidInput(format!(
            "`{}` is outside the prefix `{}`, so no rule can name it",
            provenance.table, rules.applied.prefix
        )));
    };
    let source = ColumnRef {
        table: name.to_string(),
        column: provenance.column.clone(),
    };
    let meta = rules
        .meta_table(&provenance.table)
        .filter(|m| m.value.eq_ignore_ascii_case(&provenance.column));
    let Some(meta) = meta else {
        return Ok(RuleDraft { source, meta: None });
    };
    let key = stored_key(session, database, meta, provenance, timeout).await?;
    Ok(RuleDraft {
        source,
        meta: Some(MetaDraft {
            table: meta.name.clone(),
            key_column: meta.key.clone(),
            key,
        }),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use tempfile::{TempDir, tempdir};

    use super::*;
    use crate::db::{Cell, ColumnInfo, FakeDatabase, ResultSet, TableStructure};
    use crate::error::ErrorKind;
    use crate::rules::ProjectsStore;
    use crate::rules::catalog::PresetCatalog;
    use crate::rules::testing::column;
    use crate::test_support::spec;
    use crate::value::{KeyPart, RowKey};

    const TIMEOUT: Duration = Duration::from_secs(5);
    const NOTES_PRESET: &str = "[preset]\nname = \"notes\"\nmarker_tables = [\"notes\"]\n\n[[meta]]\ntable = \"notes\"\nkey = \"k\"\nvalue = \"v\"\n";

    fn structure(columns: &[&str], primary_key: &str) -> TableStructure {
        let column = |name: &&str| ColumnInfo {
            name: name.to_string(),
            column_type: "bigint".into(),
            nullable: false,
            default: None,
            key: String::new(),
            extra: String::new(),
            comment: String::new(),
            auto_increment: false,
            generated: false,
        };
        TableStructure {
            columns: columns.iter().map(column).collect(),
            indexes: Vec::new(),
            primary_key: vec![primary_key.into()],
            foreign_keys: Vec::new(),
            ddl: None,
        }
    }

    /// A session over a fake `wp` database with `f_notes` (a `[[meta]]` table of the `notes`
    /// preset), `f_threads` and `x_stray`, which lies outside the prefix `f_`.
    struct Rig {
        session: Session,
        catalog: PresetCatalog,
        projects: ProjectsStore,
        _dir: TempDir,
    }

    impl Rig {
        fn new(db: FakeDatabase) -> Self {
            let dir = tempdir().unwrap();
            let presets = dir.path().join("presets");
            fs::create_dir_all(&presets).unwrap();
            fs::write(presets.join("notes.toml"), NOTES_PRESET).unwrap();
            let db = db
                .with_database("wp")
                .with_table(
                    "wp",
                    "f_notes",
                    structure(&["meta_id", "k", "v"], "meta_id"),
                )
                .with_table(
                    "wp",
                    "f_threads",
                    structure(&["thread_id", "status"], "thread_id"),
                )
                .with_table("wp", "x_stray", structure(&["stray_id"], "stray_id"));
            let session = Session::new(
                "s".into(),
                spec("127.0.0.1", 3306, "wp"),
                "local".into(),
                Box::new(Arc::new(db)),
            );
            Self {
                session,
                catalog: PresetCatalog::with_built_in(presets, &[]),
                projects: ProjectsStore::open(dir.path()),
                _dir: dir,
            }
        }

        async fn draft(&self, provenance: &Provenance) -> AppResult<RuleDraft> {
            let sources = RuleSources {
                presets: &self.catalog,
                projects: &self.projects,
            };
            rule_draft(&self.session, "wp", provenance, sources, TIMEOUT).await
        }
    }

    fn provenance(table: &str, column: &str, key: &[(&str, &str)]) -> Provenance {
        Provenance {
            table: table.into(),
            column: column.into(),
            row: RowKey(
                key.iter()
                    .map(|(column, value)| KeyPart {
                        column: column.to_string(),
                        value: value.to_string(),
                    })
                    .collect(),
            ),
        }
    }

    fn one_text(value: &str) -> ResultSet {
        ResultSet {
            rows: vec![vec![Cell::Text(value.into())]],
            ..ResultSet::default()
        }
    }

    #[tokio::test]
    async fn a_meta_value_returns_the_meta_table_and_the_key_stored_in_its_row() {
        let rig = Rig::new(FakeDatabase::new().on("SELECT `k` FROM", one_text("_status")));

        let draft = rig
            .draft(&provenance("f_notes", "v", &[("meta_id", "7")]))
            .await
            .unwrap();

        assert_eq!(
            draft,
            RuleDraft {
                source: column("notes.v"),
                meta: Some(MetaDraft {
                    table: "notes".into(),
                    key_column: "k".into(),
                    key: "_status".into(),
                }),
            }
        );
    }

    #[tokio::test]
    async fn a_plain_column_returns_its_rule_name_and_no_meta() {
        let rig = Rig::new(FakeDatabase::new());

        let cases = [
            (
                provenance("f_threads", "status", &[("thread_id", "1")]),
                "threads.status",
            ),
            (provenance("f_notes", "k", &[("meta_id", "7")]), "notes.k"),
        ];
        for (provenance, source) in cases {
            let draft = rig.draft(&provenance).await.unwrap();

            assert_eq!(
                draft,
                RuleDraft {
                    source: column(source),
                    meta: None,
                },
                "{source}"
            );
        }
    }

    #[tokio::test]
    async fn a_table_outside_the_prefix_is_invalid_input() {
        let rig = Rig::new(FakeDatabase::new());

        let err = rig
            .draft(&provenance("x_stray", "stray_id", &[("stray_id", "1")]))
            .await
            .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(
            matches!(&err, AppError::InvalidInput(m) if m.contains("`x_stray` is outside the prefix `f_`")),
            "{err:?}"
        );
    }
}
