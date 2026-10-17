//! A stored value as a SQL condition (`value.copyCondition`), built from its provenance through
//! `ColumnEquals`, so quoting follows the server's dialect.

use std::time::Duration;

use super::Provenance;
use crate::db::browse::{equals_sql, select_column_sql};
use crate::db::{ColumnEquals, Dialect};
use crate::error::{AppError, AppResult};
use crate::rules::effective::{self, MetaRef, RuleSources};
use crate::session::Session;

/// The key column of a `[[meta]]` table and the key the value's row is stored under.
#[derive(Debug, Clone, Copy)]
struct MetaKeyMatch<'a> {
    column: &'a str,
    key: &'a str,
}

/// `column = 'raw'` (`IS NULL` for `None`); a meta value also matches its key:
/// `meta_key = '…' AND meta_value = '…'`.
fn condition_sql(
    dialect: Dialect,
    column: &str,
    raw: Option<&str>,
    meta: Option<MetaKeyMatch>,
) -> String {
    let mut matches = Vec::with_capacity(2);
    if let Some(meta) = meta {
        matches.push(ColumnEquals {
            column: meta.column.to_string(),
            value: Some(meta.key.to_string()),
        });
    }
    matches.push(ColumnEquals {
        column: column.to_string(),
        value: raw.map(str::to_string),
    });
    equals_sql(dialect, &matches)
}

/// The condition for the value at `provenance`. On the value column of a `[[meta]]` table the
/// row's key is read from the server, since the provenance names only the row.
pub async fn value_condition(
    session: &Session,
    database: &str,
    provenance: &Provenance,
    raw: Option<&str>,
    sources: RuleSources<'_>,
    timeout: Duration,
) -> AppResult<String> {
    let dialect = session.db.dialect();
    let (_, rules) = effective::load(session, database, sources).await?;
    let meta = rules
        .meta_table(&provenance.table)
        .filter(|m| m.value.eq_ignore_ascii_case(&provenance.column));
    let Some(meta) = meta else {
        return Ok(condition_sql(dialect, &provenance.column, raw, None));
    };
    let key = stored_key(session, database, meta, provenance, timeout).await?;
    let meta_key = MetaKeyMatch {
        column: &meta.key,
        key: &key,
    };
    Ok(condition_sql(
        dialect,
        &provenance.column,
        raw,
        Some(meta_key),
    ))
}

/// The key the meta row at `provenance` is stored under, read from the server.
pub async fn stored_key(
    session: &Session,
    database: &str,
    meta: &MetaRef,
    provenance: &Provenance,
    timeout: Duration,
) -> AppResult<String> {
    let row: Vec<ColumnEquals> = provenance
        .row
        .0
        .iter()
        .map(|part| ColumnEquals {
            column: part.column.clone(),
            value: Some(part.value.clone()),
        })
        .collect();
    if row.is_empty() {
        return Err(AppError::InvalidInput(format!(
            "{} has no row key",
            provenance.table
        )));
    }
    let sql = select_column_sql(session.db.dialect(), database, &meta.table, &meta.key, &row);
    let result = session.db.fetch(&sql, timeout).await?;
    result
        .rows
        .first()
        .and_then(|cells| cells.first())
        .and_then(|cell| cell.as_text())
        .map(str::to_string)
        .ok_or_else(|| {
            AppError::InvalidInput(format!("The row of this value is gone from {}", meta.table))
        })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use tempfile::{TempDir, tempdir};

    use super::*;
    use crate::connection::ConnectionSpec;
    use crate::db::{Cell, ColumnInfo, FakeDatabase, ResultSet, TableStructure};
    use crate::error::ErrorKind;
    use crate::rules::ProjectsStore;
    use crate::rules::catalog::PresetCatalog;
    use crate::value::{KeyPart, RowKey};

    const TIMEOUT: Duration = Duration::from_secs(5);
    const NO_BACKSLASH: Dialect = Dialect {
        backslash_escapes: false,
    };
    const NOTES_PRESET: &str = "[preset]\nname = \"notes\"\nmarker_tables = [\"notes\"]\n\n[[meta]]\ntable = \"notes\"\nkey = \"k\"\nvalue = \"v\"\n";

    #[test]
    fn a_plain_column_is_compared_to_the_quoted_value() {
        assert_eq!(
            condition_sql(Dialect::MYSQL, "status", Some("wc-failed"), None),
            "`status` = 'wc-failed'"
        );
    }

    #[test]
    fn a_null_value_is_compared_with_is_null() {
        assert_eq!(
            condition_sql(Dialect::MYSQL, "status", None, None),
            "`status` IS NULL"
        );
    }

    #[test]
    fn a_meta_value_also_matches_its_key() {
        let meta = MetaKeyMatch {
            column: "meta_key",
            key: "_k",
        };

        assert_eq!(
            condition_sql(Dialect::MYSQL, "meta_value", Some("v"), Some(meta)),
            "`meta_key` = '_k' AND `meta_value` = 'v'"
        );
        assert_eq!(
            condition_sql(Dialect::MYSQL, "meta_value", None, Some(meta)),
            "`meta_key` = '_k' AND `meta_value` IS NULL"
        );
    }

    #[test]
    fn quoting_follows_the_dialect() {
        for (dialect, expected) in [
            (Dialect::MYSQL, r"`c` = 'it''s\\x'"),
            (NO_BACKSLASH, r"`c` = 'it''s\x'"),
        ] {
            assert_eq!(
                condition_sql(dialect, "c", Some(r"it's\x"), None),
                expected,
                "{dialect:?}"
            );
        }
    }

    fn column(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.to_string(),
            column_type: "bigint".into(),
            nullable: false,
            default: None,
            key: String::new(),
            extra: String::new(),
            comment: String::new(),
            auto_increment: false,
            generated: false,
        }
    }

    fn structure(columns: &[&str], primary_key: &str) -> TableStructure {
        TableStructure {
            columns: columns.iter().map(|c| column(c)).collect(),
            indexes: Vec::new(),
            primary_key: vec![primary_key.into()],
            foreign_keys: Vec::new(),
            ddl: None,
        }
    }

    /// A session over a fake `wp` database with `f_notes` (a `[[meta]]` table of the `notes`
    /// preset) and `f_threads`.
    struct Rig {
        db: Arc<FakeDatabase>,
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
            let db = Arc::new(
                db.with_database("wp")
                    .with_table(
                        "wp",
                        "f_notes",
                        structure(&["meta_id", "k", "v"], "meta_id"),
                    )
                    .with_table(
                        "wp",
                        "f_threads",
                        structure(&["thread_id", "status"], "thread_id"),
                    ),
            );
            let connection = ConnectionSpec {
                name: None,
                host: "127.0.0.1".into(),
                port: 3306,
                socket: None,
                user: "root".into(),
                database: Some("wp".into()),
            };
            let session =
                Session::new("s".into(), connection, "local".into(), Box::new(db.clone()));
            Self {
                db,
                session,
                catalog: PresetCatalog::with_built_in(presets, &[]),
                projects: ProjectsStore::open(dir.path()),
                _dir: dir,
            }
        }

        async fn condition(&self, provenance: &Provenance, raw: Option<&str>) -> AppResult<String> {
            let sources = RuleSources {
                presets: &self.catalog,
                projects: &self.projects,
            };
            value_condition(&self.session, "wp", provenance, raw, sources, TIMEOUT).await
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
    async fn a_meta_value_reads_its_key_from_the_row_and_matches_both() {
        let rig = Rig::new(FakeDatabase::new().on("SELECT `k` FROM", one_text("_status")));

        let sql = rig
            .condition(
                &provenance("f_notes", "v", &[("meta_id", "7")]),
                Some("failed"),
            )
            .await
            .unwrap();

        assert_eq!(sql, "`k` = '_status' AND `v` = 'failed'");
        assert_eq!(
            rig.db.queries(),
            ["SELECT `k` FROM `wp`.`f_notes` WHERE `meta_id` = '7' LIMIT 1"]
        );
    }

    #[tokio::test]
    async fn the_value_column_is_matched_case_insensitively_and_keeps_the_provenance_spelling() {
        let rig = Rig::new(FakeDatabase::new().on("SELECT `k` FROM", one_text("it's")));

        let sql = rig
            .condition(&provenance("f_notes", "V", &[("meta_id", "7")]), None)
            .await
            .unwrap();

        assert_eq!(sql, "`k` = 'it''s' AND `V` IS NULL");
    }

    #[tokio::test]
    async fn values_outside_a_meta_value_column_read_nothing_from_the_server() {
        let rig = Rig::new(FakeDatabase::new());
        let cases = [
            (
                provenance("f_threads", "status", &[("thread_id", "1")]),
                "`status` = 'open'",
            ),
            (
                provenance("f_notes", "k", &[("meta_id", "7")]),
                "`k` = 'open'",
            ),
            (
                provenance("f_threads", "v", &[("thread_id", "1")]),
                "`v` = 'open'",
            ),
        ];

        for (provenance, expected) in cases {
            let sql = rig.condition(&provenance, Some("open")).await.unwrap();

            assert_eq!(sql, expected, "{}.{}", provenance.table, provenance.column);
        }
        assert!(rig.db.queries().is_empty());
    }

    #[tokio::test]
    async fn a_meta_value_whose_row_is_gone_is_invalid_input() {
        let rig = Rig::new(FakeDatabase::new().on("SELECT `k` FROM", ResultSet::default()));

        let err = rig
            .condition(&provenance("f_notes", "v", &[("meta_id", "7")]), Some("x"))
            .await
            .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }

    #[tokio::test]
    async fn a_meta_value_without_a_row_key_is_invalid_input() {
        let rig = Rig::new(FakeDatabase::new());

        let err = rig
            .condition(&provenance("f_notes", "v", &[]), Some("x"))
            .await
            .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(rig.db.queries().is_empty());
    }

    #[tokio::test]
    async fn a_failing_key_read_surfaces_its_error() {
        let rig = Rig::new(FakeDatabase::new().failing("SELECT `k` FROM", AppError::Cancelled));

        let err = rig
            .condition(&provenance("f_notes", "v", &[("meta_id", "7")]), Some("x"))
            .await
            .unwrap_err();

        assert!(matches!(err, AppError::Cancelled), "{err:?}");
    }
}
