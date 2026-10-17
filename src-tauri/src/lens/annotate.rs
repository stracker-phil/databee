//! Column kinds, row keys and serialized cells for the results the session hands to the page.
//! These run before the session moves long texts into its cell cache (`find_serialized` needs
//! them in full).

use std::collections::HashMap;
use std::sync::Arc;

use super::{Lens, SchemaLens};
use crate::db::{BrowsePage, ColumnMeta, ExecuteResponse, ResultSet};
use crate::error::AppResult;
use crate::schema::{self, DatabaseSchema};
use crate::session::Session;
use crate::value::{self, ColumnKind};

/// Kinds from the lens of the browsed table, and the rows' keys. Without a schema (it failed to
/// load) the page still shows, with kinds from the SQL types.
pub fn annotate_browse(
    session: &Session,
    page: &mut BrowsePage,
    database: &str,
    schema: AppResult<Arc<DatabaseSchema>>,
) {
    value::find_serialized(&mut page.result);
    let schema = match schema {
        Ok(schema) => schema,
        Err(e) => {
            log::warn!("Schema of {database} not loaded, browsing without references: {e}");
            page.result.kinds = type_kinds(&page.result.columns);
            return;
        }
    };
    let primary_key = schema
        .keys(&page.table)
        .map(|k| k.primary_key.clone())
        .unwrap_or_default();
    let lens = SchemaLens::new(schema, &session.slug(), database);
    page.result.kinds = lens.kinds(&page.table, &page.result.columns);
    page.keys = value::row_keys(&page.result, &primary_key);
}

/// Kinds from the SQL types, and from the lens of each column's origin table where the server
/// names one, so `SELECT post_author FROM wp_posts` shows references too.
pub async fn annotate_execute(session: &Session, response: &mut ExecuteResponse) {
    // `None` remembers a database whose schema failed to load, so it is tried once per call.
    let mut lenses = HashMap::<String, Option<SchemaLens>>::new();
    for result in &mut response.results {
        value::find_serialized(result);
        result.kinds = type_kinds(&result.columns);
        annotate_origins(session, response.database.as_deref(), result, &mut lenses).await;
    }
}

async fn annotate_origins(
    session: &Session,
    current: Option<&str>,
    result: &mut ResultSet,
    lenses: &mut HashMap<String, Option<SchemaLens>>,
) {
    for (col, column) in result.columns.iter().enumerate() {
        let Some(origin) = &column.origin else {
            continue;
        };
        let Some(database) = origin.database.as_deref().or(current) else {
            continue;
        };
        if !lenses.contains_key(database) {
            let lens = load(session, database).await;
            lenses.insert(database.to_string(), lens);
        }
        let Some(Some(lens)) = lenses.get(database) else {
            continue;
        };
        // The lens knows the column by its real name, not the alias the result shows.
        let real = ColumnMeta {
            name: origin.column.clone(),
            ..column.clone()
        };
        if let Some(kind) = lens.kinds(&origin.table, &[real]).into_iter().next() {
            result.kinds[col] = kind;
        }
    }
}

async fn load(session: &Session, database: &str) -> Option<SchemaLens> {
    match schema::database(session, database).await {
        Ok(schema) => Some(SchemaLens::new(schema, &session.slug(), database)),
        Err(e) => {
            log::debug!("Schema of {database} not loaded for result kinds: {e}");
            None
        }
    }
}

fn type_kinds(columns: &[ColumnMeta]) -> Vec<ColumnKind> {
    columns
        .iter()
        .map(|c| value::type_kind(&c.type_name))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::address::{Address, Target};
    use crate::connection::ConnectionSpec;
    use crate::db::{
        BrowseRequest, Cell, CellLimit, ColumnInfo, ColumnOrigin, FakeDatabase, TableStructure,
    };
    use crate::decode::value::Format;
    use crate::error::AppError;
    use crate::value::{KeyPart, ReferenceSource, ReferenceTarget, RowKey, SerializedCell};

    const TIMEOUT: Duration = Duration::from_secs(5);

    fn column(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
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

    fn table(columns: &[&str], primary_key: &str) -> TableStructure {
        TableStructure {
            columns: columns.iter().map(|c| column(c)).collect(),
            indexes: Vec::new(),
            primary_key: vec![primary_key.into()],
            foreign_keys: Vec::new(),
            ddl: None,
        }
    }

    fn spec() -> ConnectionSpec {
        ConnectionSpec {
            name: None,
            host: "127.0.0.1".into(),
            port: 3306,
            socket: None,
            user: "root".into(),
            database: Some("wp".into()),
        }
    }

    fn session_over(db: FakeDatabase) -> Session {
        let db = db
            .with_database("wp")
            .with_table("wp", "owners", table(&["owner_id", "name"], "owner_id"))
            .with_table("wp", "items", table(&["item_id", "owner_id"], "item_id"));
        Session::new("s".into(), spec(), "local".into(), Box::new(db))
    }

    fn column_meta(name: &str, type_name: &str) -> ColumnMeta {
        ColumnMeta {
            name: name.into(),
            type_name: type_name.into(),
            origin: None,
        }
    }

    fn table_address(table: &str) -> Address {
        let target = Target::Table {
            table: table.into(),
            filter: Vec::new(),
        };
        Address::new("local", "wp", target)
    }

    fn items_page_result() -> ResultSet {
        ResultSet {
            columns: vec![
                column_meta("item_id", "BIGINT"),
                column_meta("owner_id", "BIGINT"),
            ],
            rows: vec![
                vec![Cell::Number("1".into()), Cell::Number("5".into())],
                vec![Cell::Number("2".into()), Cell::Number("0".into())],
            ],
            ..ResultSet::default()
        }
    }

    fn items_request() -> BrowseRequest {
        BrowseRequest {
            database: "wp".into(),
            table: "items".into(),
            limit: 10,
            ..BrowseRequest::default()
        }
    }

    fn items_page(result: ResultSet) -> BrowsePage {
        BrowsePage {
            table: "items".into(),
            result,
            keys: Vec::new(),
            has_more: false,
            limit: 10,
            filter: None,
            columns_filter: None,
        }
    }

    fn item_key(value: &str) -> RowKey {
        RowKey(vec![KeyPart {
            column: "item_id".into(),
            value: value.into(),
        }])
    }

    fn items_kinds() -> Vec<ColumnKind> {
        vec![
            ColumnKind::PrimaryKey {
                table: table_address("items"),
                columns: vec!["item_id".into()],
            },
            ColumnKind::Reference {
                targets: vec![ReferenceTarget {
                    table: table_address("owners"),
                    column: "owner_id".into(),
                    key: true,
                }],
                by: ReferenceSource::SameName {
                    column: "owner_id".into(),
                    indexed: false,
                },
            },
        ]
    }

    #[tokio::test]
    async fn browse_fills_column_kinds_and_row_keys() {
        let session = session_over(FakeDatabase::new().on("items", items_page_result()));

        let page = session
            .browse(&items_request(), CellLimit::NONE, TIMEOUT)
            .await
            .unwrap();

        assert_eq!(page.result.kinds, items_kinds());
        assert_eq!(page.keys, [item_key("1"), item_key("2")]);
    }

    #[test]
    fn browse_without_a_schema_still_shows_the_page_with_type_kinds() {
        let session = session_over(FakeDatabase::new());
        let mut result = items_page_result();
        result.columns.push(column_meta("meta", "VARCHAR"));
        result.rows[0].push(Cell::Text("a:1:{i:0;i:1;}".into()));
        result.rows[1].push(Cell::Text("plain".into()));
        let mut page = items_page(result);

        annotate_browse(
            &session,
            &mut page,
            "wp",
            Err(AppError::InvalidInput("no schema".into())),
        );

        assert_eq!(
            page.result.kinds,
            [ColumnKind::Number, ColumnKind::Number, ColumnKind::Text]
        );
        assert!(page.keys.is_empty());
        assert_eq!(
            page.result.serialized,
            [SerializedCell {
                row: 0,
                col: 2,
                format: Format::Php,
                preview: "a:1:{…}".into(),
            }]
        );
    }

    #[tokio::test]
    async fn execute_gives_a_column_the_kind_of_its_origin_also_under_an_alias() {
        let mut aliased = column_meta("o", "BIGINT");
        aliased.origin = Some(ColumnOrigin {
            database: None,
            table: "items".into(),
            column: "owner_id".into(),
        });
        let result = ResultSet {
            columns: vec![aliased, column_meta("n", "VARCHAR")],
            rows: vec![vec![Cell::Number("5".into()), Cell::Text("x".into())]],
            returns_rows: true,
            ..ResultSet::default()
        };
        let session = session_over(FakeDatabase::new().on("SELECT", result));

        let response = session
            .execute(
                "SELECT owner_id AS o, 'x' AS n FROM items",
                10,
                CellLimit::NONE,
                TIMEOUT,
            )
            .await
            .unwrap();

        assert_eq!(
            response.results[0].kinds,
            [items_kinds().remove(1), ColumnKind::Text]
        );
    }

    #[tokio::test]
    async fn execute_without_origins_uses_the_sql_types() {
        let result = ResultSet {
            columns: vec![
                column_meta("n", "BIGINT"),
                column_meta("t", "DATETIME"),
                column_meta("s", "VARCHAR"),
            ],
            rows: vec![vec![
                Cell::Number("1".into()),
                Cell::Temporal("2026-01-01 00:00:00".into()),
                Cell::Text("x".into()),
            ]],
            returns_rows: true,
            ..ResultSet::default()
        };
        let session = session_over(FakeDatabase::new().on("SELECT", result));

        let response = session
            .execute("SELECT 1, NOW(), 'x'", 10, CellLimit::NONE, TIMEOUT)
            .await
            .unwrap();

        assert_eq!(
            response.results[0].kinds,
            [ColumnKind::Number, ColumnKind::Timestamp, ColumnKind::Text]
        );
    }

    async fn fixture_sessions(database: &str) -> Vec<(String, Session)> {
        let mut out = Vec::new();
        for (host, port) in crate::test_support::servers() {
            let db = crate::test_support::connect_to(&host, port, database).await;
            let spec = crate::test_support::spec(&host, port, database);
            let session = Session::new("it".into(), spec, format!("it-{port}"), Box::new(db));
            out.push((format!("{host}:{port}"), session));
        }
        out
    }

    #[tokio::test]
    #[ignore]
    async fn execute_reports_the_origin_of_an_aliased_column_on_the_fixture() {
        for (server, session) in fixture_sessions("wp_test").await {
            let response = session
                .execute(
                    "SELECT post_author AS a FROM wp_test.wp_posts",
                    10,
                    CellLimit::NONE,
                    TIMEOUT,
                )
                .await
                .unwrap_or_else(|e| panic!("{server}: {e:?}"));

            let column = &response.results[0].columns[0];
            assert_eq!(column.name, "a", "{server}");
            assert_eq!(
                column.origin,
                Some(ColumnOrigin {
                    database: Some("wp_test".into()),
                    table: "wp_posts".into(),
                    column: "post_author".into(),
                }),
                "{server}"
            );
        }
    }

    #[tokio::test]
    #[ignore]
    async fn browse_gives_every_row_a_key_and_marks_the_primary_key_on_the_fixture() {
        for (server, session) in fixture_sessions("wp_test").await {
            let request = BrowseRequest {
                database: "wp_test".into(),
                table: "wp_posts".into(),
                limit: 50,
                ..BrowseRequest::default()
            };

            let page = session
                .browse(&request, CellLimit::NONE, TIMEOUT)
                .await
                .unwrap_or_else(|e| panic!("{server}: {e:?}"));

            assert_eq!(page.keys.len(), page.result.rows.len(), "{server}");
            let id = page
                .result
                .columns
                .iter()
                .position(|c| c.name == "ID")
                .unwrap();
            assert!(
                matches!(
                    &page.result.kinds[id],
                    ColumnKind::PrimaryKey { columns, .. } if columns == &["ID".to_string()]
                ),
                "{server}: {:?}",
                page.result.kinds[id]
            );
        }
    }
}
