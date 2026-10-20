//! The live check of a table tab: the rows a page shows, read again by primary key as one
//! checksum per row, and the rows whose checksum moved read in full to say which values changed.

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use crate::db::{Cell, CellLimit, Dialect, ResultSet};
use crate::error::{AppError, AppResult};
use crate::session::Session;
use crate::state::AppState;
use crate::value::RowKey;

/// A check runs every few seconds; one that takes longer is not worth waiting for.
const CHECK_TIMEOUT: Duration = Duration::from_secs(3);
/// The largest page a table tab shows (`db::browse`).
const MAX_ROWS: usize = 500;

/// The rows of a page by primary key, and the page's columns the checksum covers.
#[derive(Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct TableCheckRequest {
    pub database: String,
    pub table: String,
    pub columns: Vec<String>,
    pub keys: Vec<RowKey>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct TableCheck {
    /// One per key, in request order: the stored row's checksum, `None` when no row has the key.
    pub hashes: Vec<Option<String>>,
}

/// Rows whose checksum moved, with the cells the page shows for them.
#[derive(Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct TableCheckRowsRequest {
    pub database: String,
    pub table: String,
    pub columns: Vec<String>,
    /// The page's result set, whose long text cells the session keeps in full.
    #[ts(type = "number")]
    pub result_id: u64,
    pub rows: Vec<ShownRow>,
}

#[derive(Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ShownRow {
    /// The row's index in the page.
    pub row: usize,
    pub key: RowKey,
    /// In `columns` order.
    pub cells: Vec<Cell>,
}

/// A shown row as it is stored now; a row that is gone is left out.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct CheckedRow {
    pub row: usize,
    /// The checksum these cells have.
    pub hash: String,
    /// In `columns` order, cut like a page's cells.
    pub cells: Vec<Cell>,
    /// The columns whose stored value differs from the shown one. Empty when only a part the
    /// page does not show changed (binary values beyond their preview).
    pub changed: Vec<usize>,
}

/// `MD5` of every column's `MD5`, `-` for NULL: 32 hex characters or `-` per column, so no two
/// rows that differ in any value (NULL and `''` included) share the text that is hashed.
fn row_hash_sql(dialect: Dialect, columns: &[String]) -> String {
    let parts: Vec<String> = columns
        .iter()
        .map(|c| format!("IFNULL(MD5({}),'-')", dialect.quote_ident(c)))
        .collect();
    format!("MD5(CONCAT({}))", parts.join(","))
}

/// `k IN ('1','2')`, or `(k1,k2) IN (('1','a'),('2','b'))` for a composite key; values are
/// quoted strings like a structured filter's, which the server compares through the key index.
fn keys_in_sql(dialect: Dialect, key_columns: &[String], keys: &[RowKey]) -> String {
    let tuple = |values: Vec<String>| match values.as_slice() {
        [one] => one.clone(),
        _ => format!("({})", values.join(",")),
    };
    let columns = tuple(key_columns.iter().map(|c| dialect.quote_ident(c)).collect());
    let values: Vec<String> = keys
        .iter()
        .map(|key| tuple(key.0.iter().map(|p| dialect.sql_string(&p.value)).collect()))
        .collect();
    format!("{columns} IN ({})", values.join(","))
}

/// The key columns of `keys`, which must all name the same columns in the same order.
fn key_columns(keys: &[RowKey]) -> AppResult<Vec<String>> {
    let Some(first) = keys.first() else {
        return Ok(Vec::new());
    };
    let columns: Vec<String> = first.0.iter().map(|p| p.column.clone()).collect();
    let same = |key: &RowKey| {
        key.0.len() == columns.len() && key.0.iter().zip(&columns).all(|(p, c)| &p.column == c)
    };
    if columns.is_empty() || !keys.iter().all(same) {
        return Err(AppError::InvalidInput(
            "Every row of a live check needs the same primary key".into(),
        ));
    }
    Ok(columns)
}

fn check_size(columns: &[String], rows: usize) -> AppResult<()> {
    if columns.is_empty() {
        return Err(AppError::InvalidInput("A live check needs columns".into()));
    }
    if rows > MAX_ROWS {
        return Err(AppError::InvalidInput(format!(
            "A live check reads at most {MAX_ROWS} rows"
        )));
    }
    Ok(())
}

/// The key columns, then `select`, of every row of `table` with one of `keys`.
fn select_by_keys(
    dialect: Dialect,
    database: &str,
    table: &str,
    key_columns: &[String],
    select: &str,
    keys: &[RowKey],
) -> String {
    let keys_select: Vec<String> = key_columns.iter().map(|c| dialect.quote_ident(c)).collect();
    format!(
        "SELECT {}, {select} FROM {} WHERE {}",
        keys_select.join(", "),
        dialect.table_ref(database, table),
        keys_in_sql(dialect, key_columns, keys)
    )
}

/// The rows of `result` by the text of their first `width` cells, the key; a row whose key is
/// not text cannot match a shown key and is left out.
fn rows_by_key(result: ResultSet, width: usize) -> HashMap<Vec<String>, Vec<Cell>> {
    result
        .rows
        .into_iter()
        .filter_map(|mut cells| {
            let rest = cells.split_off(width.min(cells.len()));
            let key = cells
                .iter()
                .map(|c| c.as_text().map(str::to_string))
                .collect::<Option<Vec<_>>>()?;
            Some((key, rest))
        })
        .collect()
}

fn key_text(key: &RowKey) -> Vec<String> {
    key.0.iter().map(|p| p.value.clone()).collect()
}

/// One statement: the checksum of every row with one of the keys.
pub(crate) async fn check(
    session: &Session,
    request: &TableCheckRequest,
    timeout: Duration,
) -> AppResult<TableCheck> {
    check_size(&request.columns, request.keys.len())?;
    let key_columns = key_columns(&request.keys)?;
    if request.keys.is_empty() {
        return Ok(TableCheck { hashes: Vec::new() });
    }
    let dialect = session.db.dialect();
    let sql = select_by_keys(
        dialect,
        &request.database,
        &request.table,
        &key_columns,
        &row_hash_sql(dialect, &request.columns),
        &request.keys,
    );
    let stored = rows_by_key(
        session.db.fetch(&sql, timeout.min(CHECK_TIMEOUT)).await?,
        key_columns.len(),
    );
    let hashes = request
        .keys
        .iter()
        .map(|key| {
            stored
                .get(&key_text(key))
                .and_then(|rest| rest.first())
                .and_then(Cell::as_text)
                .map(str::to_string)
        })
        .collect();
    Ok(TableCheck { hashes })
}

/// Whether the stored `new` cell holds what the page shows as `old`. A long text compares in
/// full when the session still keeps it (`old_full`), else by length and preview.
fn same_value(old: &Cell, old_full: Option<&str>, new: &Cell) -> bool {
    match (old, new) {
        (Cell::LongText { len, preview }, Cell::Text(text)) => match old_full {
            Some(full) => full == text,
            None => text.chars().count() as u64 == *len && text.starts_with(preview.as_str()),
        },
        _ => old == new,
    }
}

/// One statement: the rows in full with their checksum, compared with the shown cells.
pub(crate) async fn changed_rows(
    session: &Session,
    request: &TableCheckRowsRequest,
    cell_limit: CellLimit,
    timeout: Duration,
) -> AppResult<Vec<CheckedRow>> {
    check_size(&request.columns, request.rows.len())?;
    let keys: Vec<RowKey> = request.rows.iter().map(|r| r.key.clone()).collect();
    let key_columns = key_columns(&keys)?;
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let dialect = session.db.dialect();
    let mut select: Vec<String> = request
        .columns
        .iter()
        .map(|c| dialect.quote_ident(c))
        .collect();
    select.push(row_hash_sql(dialect, &request.columns));
    let sql = select_by_keys(
        dialect,
        &request.database,
        &request.table,
        &key_columns,
        &select.join(", "),
        &keys,
    );
    let mut stored = rows_by_key(
        session.db.fetch(&sql, timeout.min(CHECK_TIMEOUT)).await?,
        key_columns.len(),
    );
    let width = request.columns.len();
    Ok(request
        .rows
        .iter()
        .filter_map(|shown| {
            let mut cells = stored.remove(&key_text(&shown.key))?;
            let hash = cells.get(width)?.as_text()?.to_string();
            cells.truncate(width);
            let changed = (0..width)
                .filter(|&col| {
                    let Some(old) = shown.cells.get(col) else {
                        return false;
                    };
                    let full = matches!(old, Cell::LongText { .. })
                        .then(|| session.cells.get(request.result_id, shown.row, col).ok())
                        .flatten();
                    !same_value(old, full.as_deref(), &cells[col])
                })
                .collect();
            Some(CheckedRow {
                row: shown.row,
                hash,
                cells: cells.into_iter().map(|c| c.limited(cell_limit)).collect(),
                changed,
            })
        })
        .collect())
}

/// The checksums of a page's rows (one statement); see `check`.
#[tauri::command]
pub async fn table_check(
    state: State<'_, AppState>,
    session_id: String,
    request: TableCheckRequest,
) -> AppResult<TableCheck> {
    let session = state.sessions.get(&session_id)?;
    check(&session, &request, state.settings.get().query_timeout()).await
}

/// Rows whose checksum moved, as stored now (one statement); see `changed_rows`.
#[tauri::command]
pub async fn table_check_rows(
    state: State<'_, AppState>,
    session_id: String,
    request: TableCheckRowsRequest,
) -> AppResult<Vec<CheckedRow>> {
    let settings = state.settings.get();
    let session = state.sessions.get(&session_id)?;
    changed_rows(
        &session,
        &request,
        settings.cell_limit(),
        settings.query_timeout(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::db::FakeDatabase;
    use crate::error::ErrorKind;
    use crate::test_support::{Counting, connect_to, servers, spec};
    use crate::value::KeyPart;

    const TIMEOUT: Duration = Duration::from_secs(10);
    const NAME_EMAIL_HASH: &str = "MD5(CONCAT(IFNULL(MD5(`name`),'-'),IFNULL(MD5(`email`),'-')))";

    fn key(parts: &[(&str, &str)]) -> RowKey {
        RowKey(
            parts
                .iter()
                .map(|(column, value)| KeyPart {
                    column: column.to_string(),
                    value: value.to_string(),
                })
                .collect(),
        )
    }

    fn id_key(id: impl ToString) -> RowKey {
        key(&[("id", &id.to_string())])
    }

    fn strings(names: &[&str]) -> Vec<String> {
        names.iter().map(ToString::to_string).collect()
    }

    fn text(value: &str) -> Cell {
        Cell::Text(value.into())
    }

    fn canned(rows: Vec<Vec<Cell>>) -> ResultSet {
        ResultSet {
            rows,
            ..ResultSet::default()
        }
    }

    fn session_over(db: &Arc<FakeDatabase>) -> Session {
        Session::new(
            "s".into(),
            spec("127.0.0.1", 3306, "wp"),
            "local".into(),
            Box::new(db.clone()),
        )
    }

    fn check_request(columns: &[&str], keys: Vec<RowKey>) -> TableCheckRequest {
        TableCheckRequest {
            database: "wp".into(),
            table: "users".into(),
            columns: strings(columns),
            keys,
        }
    }

    fn shown(row: usize, id: u32, cells: Vec<Cell>) -> ShownRow {
        ShownRow {
            row,
            key: id_key(id),
            cells,
        }
    }

    fn rows_request(columns: &[&str], rows: Vec<ShownRow>) -> TableCheckRowsRequest {
        TableCheckRowsRequest {
            database: "wp".into(),
            table: "users".into(),
            columns: strings(columns),
            result_id: 7,
            rows,
        }
    }

    async fn hashes(db: FakeDatabase, request: &TableCheckRequest) -> Vec<Option<String>> {
        let db = Arc::new(db);
        check(&session_over(&db), request, TIMEOUT)
            .await
            .unwrap()
            .hashes
    }

    async fn changed(
        db: FakeDatabase,
        session_setup: impl FnOnce(&Session),
        request: &TableCheckRowsRequest,
        limit: CellLimit,
    ) -> Vec<CheckedRow> {
        let db = Arc::new(db);
        let session = session_over(&db);
        session_setup(&session);
        changed_rows(&session, request, limit, TIMEOUT)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn single_key_check_selects_the_hash_of_every_column_by_key() {
        let db = Arc::new(FakeDatabase::new().on("FROM", canned(Vec::new())));
        let request = check_request(&["name", "email"], vec![id_key(1), id_key(2)]);

        check(&session_over(&db), &request, TIMEOUT).await.unwrap();

        assert_eq!(
            db.queries(),
            [format!(
                "SELECT `id`, {NAME_EMAIL_HASH} FROM `wp`.`users` WHERE `id` IN ('1','2')"
            )]
        );
    }

    #[tokio::test]
    async fn composite_key_check_compares_key_tuples() {
        let db = Arc::new(FakeDatabase::new().on("FROM", canned(Vec::new())));
        let request = TableCheckRequest {
            table: "meta".into(),
            ..check_request(
                &["value"],
                vec![
                    key(&[("post_id", "1"), ("meta_key", "a")]),
                    key(&[("post_id", "2"), ("meta_key", "b")]),
                ],
            )
        };

        check(&session_over(&db), &request, TIMEOUT).await.unwrap();

        assert_eq!(
            db.queries(),
            [
                "SELECT `post_id`, `meta_key`, MD5(CONCAT(IFNULL(MD5(`value`),'-'))) \
                 FROM `wp`.`meta` WHERE (`post_id`,`meta_key`) IN (('1','a'),('2','b'))"
            ]
        );
    }

    #[tokio::test]
    async fn check_quotes_identifiers_and_escapes_key_values() {
        let db = Arc::new(FakeDatabase::new().on("FROM", canned(Vec::new())));
        let request = TableCheckRequest {
            database: "w`p".into(),
            table: "us`ers".into(),
            columns: strings(&["na`me"]),
            keys: vec![key(&[("i`d", "it's\\a\nb")])],
        };

        check(&session_over(&db), &request, TIMEOUT).await.unwrap();

        assert_eq!(
            db.queries(),
            ["SELECT `i``d`, MD5(CONCAT(IFNULL(MD5(`na``me`),'-'))) \
                 FROM `w``p`.`us``ers` WHERE `i``d` IN ('it''s\\\\a\\nb')"]
        );
    }

    #[tokio::test]
    async fn check_sends_one_statement_whatever_the_page_size() {
        let (counting, log) =
            Counting::new(Box::new(FakeDatabase::new().on("FROM", canned(Vec::new()))));
        let session = Session::new(
            "s".into(),
            spec("127.0.0.1", 3306, "wp"),
            "local".into(),
            Box::new(counting),
        );
        let request = check_request(&["name"], (1..=300).map(id_key).collect());

        check(&session, &request, TIMEOUT).await.unwrap();

        assert_eq!(log.take().len(), 1);
    }

    #[tokio::test]
    async fn check_without_keys_sends_nothing_and_returns_no_hashes() {
        let db = Arc::new(FakeDatabase::new());
        let request = check_request(&["name"], Vec::new());

        let result = check(&session_over(&db), &request, TIMEOUT).await.unwrap();

        assert_eq!(result, TableCheck { hashes: Vec::new() });
        assert!(db.queries().is_empty());
    }

    #[tokio::test]
    async fn hashes_follow_the_request_order_and_a_missing_key_is_none() {
        let db = FakeDatabase::new().on(
            "FROM",
            canned(vec![
                vec![Cell::Number("3".into()), text("hash3")],
                vec![Cell::Number("1".into()), text("hash1")],
            ]),
        );
        let request = check_request(&["name"], vec![id_key(1), id_key(2), id_key(3)]);

        assert_eq!(
            hashes(db, &request).await,
            [Some("hash1".to_string()), None, Some("hash3".to_string())]
        );
    }

    #[tokio::test]
    async fn stored_keys_match_shown_keys_by_exact_text() {
        let db = FakeDatabase::new().on(
            "FROM",
            canned(vec![
                vec![text("a"), text("hash-a")],
                vec![text("A "), text("hash-other")],
            ]),
        );
        let request = check_request(
            &["name"],
            vec![key(&[("slug", "a")]), key(&[("slug", "A")])],
        );

        assert_eq!(
            hashes(db, &request).await,
            [Some("hash-a".to_string()), None]
        );
    }

    #[tokio::test]
    async fn composite_hashes_match_on_every_key_part() {
        let db = FakeDatabase::new().on(
            "FROM",
            canned(vec![vec![text("1"), text("b"), text("hash")]]),
        );
        let request = check_request(
            &["value"],
            vec![
                key(&[("post_id", "1"), ("meta_key", "a")]),
                key(&[("post_id", "1"), ("meta_key", "b")]),
            ],
        );

        assert_eq!(hashes(db, &request).await, [None, Some("hash".to_string())]);
    }

    #[tokio::test]
    async fn requests_that_cannot_be_checked_are_invalid_input_and_send_nothing() {
        let many: Vec<RowKey> = (0..=500).map(id_key).collect();
        let cases = [
            (
                "keys of different columns",
                check_request(&["name"], vec![id_key(1), key(&[("ID", "2")])]),
            ),
            (
                "keys of different widths",
                check_request(&["name"], vec![id_key(1), key(&[("id", "2"), ("x", "3")])]),
            ),
            (
                "a key without parts",
                check_request(&["name"], vec![RowKey(Vec::new())]),
            ),
            ("no columns", check_request(&[], vec![id_key(1)])),
            ("no columns and no keys", check_request(&[], Vec::new())),
            ("more than 500 rows", check_request(&["name"], many)),
        ];
        for (what, request) in cases {
            let db = Arc::new(FakeDatabase::new());

            let err = check(&session_over(&db), &request, TIMEOUT)
                .await
                .unwrap_err();

            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{what}");
            assert!(db.queries().is_empty(), "{what}");
        }
    }

    #[tokio::test]
    async fn a_page_of_500_rows_is_checked() {
        let db = FakeDatabase::new().on("FROM", canned(Vec::new()));
        let request = check_request(&["name"], (0..500).map(id_key).collect());

        assert_eq!(hashes(db, &request).await.len(), 500);
    }

    #[tokio::test]
    async fn server_errors_pass_through() {
        let db = Arc::new(FakeDatabase::new().failing("FROM", AppError::Cancelled));
        let request = check_request(&["name"], vec![id_key(1)]);

        let err = check(&session_over(&db), &request, TIMEOUT)
            .await
            .unwrap_err();

        assert!(matches!(err, AppError::Cancelled), "{err:?}");
    }

    #[tokio::test]
    async fn the_timeout_is_capped_at_three_seconds() {
        for (given, used) in [
            (Duration::from_secs(30), Duration::from_secs(3)),
            (Duration::from_secs(1), Duration::from_secs(1)),
        ] {
            let db = Arc::new(FakeDatabase::new().on("FROM", canned(Vec::new())));
            let request = check_request(&["name"], vec![id_key(1)]);

            check(&session_over(&db), &request, given).await.unwrap();

            assert_eq!(db.fetch_timeouts(), [used], "{given:?}");
        }
    }

    #[tokio::test]
    async fn changed_rows_selects_key_columns_and_hash_in_one_statement() {
        let (counting, log) =
            Counting::new(Box::new(FakeDatabase::new().on("FROM", canned(Vec::new()))));
        let session = Session::new(
            "s".into(),
            spec("127.0.0.1", 3306, "wp"),
            "local".into(),
            Box::new(counting),
        );
        let request = rows_request(
            &["name", "email"],
            vec![
                shown(0, 1, vec![text("a"), text("b")]),
                shown(1, 2, vec![text("a"), text("b")]),
            ],
        );

        changed_rows(&session, &request, CellLimit::NONE, TIMEOUT)
            .await
            .unwrap();

        assert_eq!(
            log.take(),
            [format!(
                "SELECT `id`, `name`, `email`, {NAME_EMAIL_HASH} FROM `wp`.`users` \
                 WHERE `id` IN ('1','2')"
            )]
        );
    }

    #[tokio::test]
    async fn changed_rows_without_rows_sends_nothing() {
        let db = Arc::new(FakeDatabase::new());
        let request = rows_request(&["name"], Vec::new());

        let rows = changed_rows(&session_over(&db), &request, CellLimit::NONE, TIMEOUT)
            .await
            .unwrap();

        assert!(rows.is_empty());
        assert!(db.queries().is_empty());
    }

    #[tokio::test]
    async fn changed_rows_rejects_what_check_rejects() {
        let many: Vec<ShownRow> = (0..=500)
            .map(|i| shown(i as usize, i, vec![text("a")]))
            .collect();
        let cases = [
            (
                "keys of different columns",
                rows_request(
                    &["name"],
                    vec![
                        shown(0, 1, vec![text("a")]),
                        ShownRow {
                            row: 1,
                            key: key(&[("ID", "2")]),
                            cells: vec![text("a")],
                        },
                    ],
                ),
            ),
            (
                "no columns",
                rows_request(&[], vec![shown(0, 1, Vec::new())]),
            ),
            ("more than 500 rows", rows_request(&["name"], many)),
        ];
        for (what, request) in cases {
            let db = Arc::new(FakeDatabase::new());

            let err = changed_rows(&session_over(&db), &request, CellLimit::NONE, TIMEOUT)
                .await
                .unwrap_err();

            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{what}");
            assert!(db.queries().is_empty(), "{what}");
        }
    }

    #[tokio::test]
    async fn changed_rows_name_the_columns_whose_cell_differs() {
        let cases = [
            ("same", text("a"), text("a"), false),
            ("text edited", text("a"), text("b"), true),
            ("text to NULL", text("a"), Cell::Null, true),
            ("NULL to empty text", Cell::Null, text(""), true),
            ("NULL stays", Cell::Null, Cell::Null, false),
            (
                "number edited",
                Cell::Number("1".into()),
                Cell::Number("2".into()),
                true,
            ),
            (
                "number to text of the same digits",
                Cell::Number("1".into()),
                text("1"),
                true,
            ),
            (
                "temporal edited",
                Cell::Temporal("2024-01-01".into()),
                Cell::Temporal("2024-01-02".into()),
                true,
            ),
        ];
        for (what, old, new, differs) in cases {
            let db = FakeDatabase::new().on(
                "FROM",
                canned(vec![vec![
                    Cell::Number("1".into()),
                    text("same"),
                    new.clone(),
                    text("hash"),
                ]]),
            );
            let request = rows_request(
                &["fixed", "probe"],
                vec![shown(4, 1, vec![text("same"), old])],
            );

            let rows = changed(db, |_| {}, &request, CellLimit::NONE).await;

            let expected: Vec<usize> = if differs { vec![1] } else { Vec::new() };
            assert_eq!(rows.len(), 1, "{what}");
            assert_eq!(rows[0].changed, expected, "{what}");
        }
    }

    #[tokio::test]
    async fn changed_row_carries_its_index_hash_and_stored_cells() {
        let db = FakeDatabase::new().on(
            "FROM",
            canned(vec![vec![
                Cell::Number("5".into()),
                text("new name"),
                Cell::Null,
                text("abc123"),
            ]]),
        );
        let request = rows_request(
            &["name", "email"],
            vec![shown(3, 5, vec![text("old name"), Cell::Null])],
        );

        let rows = changed(db, |_| {}, &request, CellLimit::NONE).await;

        assert_eq!(
            rows,
            [CheckedRow {
                row: 3,
                hash: "abc123".into(),
                cells: vec![text("new name"), Cell::Null],
                changed: vec![0],
            }]
        );
    }

    #[tokio::test]
    async fn changed_rows_follow_the_shown_order_and_leave_out_missing_rows() {
        let db = FakeDatabase::new().on(
            "FROM",
            canned(vec![
                vec![Cell::Number("3".into()), text("c"), text("h3")],
                vec![Cell::Number("1".into()), text("a"), text("h1")],
            ]),
        );
        let request = rows_request(
            &["name"],
            vec![
                shown(0, 1, vec![text("a")]),
                shown(1, 2, vec![text("b")]),
                shown(2, 3, vec![text("x")]),
            ],
        );

        let rows = changed(db, |_| {}, &request, CellLimit::NONE).await;

        let seen: Vec<(usize, &str)> = rows.iter().map(|r| (r.row, r.hash.as_str())).collect();
        assert_eq!(seen, [(0, "h1"), (2, "h3")]);
    }

    #[tokio::test]
    async fn long_text_is_compared_against_the_full_text_the_session_keeps() {
        let shown_cell = Cell::LongText {
            len: 9,
            preview: "full".into(),
        };
        for (stored, differs) in [
            ("full text", false),
            ("full tEXT", true),
            ("full text!", true),
        ] {
            let db = FakeDatabase::new().on(
                "FROM",
                canned(vec![vec![
                    Cell::Number("1".into()),
                    text(stored),
                    text("h"),
                ]]),
            );
            let request = rows_request(&["body"], vec![shown(2, 1, vec![shown_cell.clone()])]);

            let rows = changed(
                db,
                |session| session.cells.insert(7, 2, 0, "full text".into()),
                &request,
                CellLimit::NONE,
            )
            .await;

            assert_eq!(rows[0].changed.is_empty(), !differs, "{stored}");
        }
    }

    #[tokio::test]
    async fn evicted_long_text_is_compared_by_length_and_preview() {
        let shown_cell = Cell::LongText {
            len: 5,
            preview: "äö".into(),
        };
        for (stored, differs) in [
            ("äöabc", false),
            ("äöxyz", false),
            ("äöabcd", true),
            ("äab", true),
            ("öäabc", true),
            ("äöabcdé", true),
        ] {
            let db = FakeDatabase::new().on(
                "FROM",
                canned(vec![vec![
                    Cell::Number("1".into()),
                    text(stored),
                    text("h"),
                ]]),
            );
            let request = rows_request(&["body"], vec![shown(0, 1, vec![shown_cell.clone()])]);

            let rows = changed(db, |_| {}, &request, CellLimit::NONE).await;

            assert_eq!(rows[0].changed.is_empty(), !differs, "{stored}");
        }
    }

    #[tokio::test]
    async fn evicted_long_text_is_unchanged_for_a_matching_short_text_and_changed_for_null() {
        let shown_cell = Cell::LongText {
            len: 3,
            preview: "ab".into(),
        };
        for stored in [text("abc"), Cell::Null] {
            let db = FakeDatabase::new().on(
                "FROM",
                canned(vec![vec![
                    Cell::Number("1".into()),
                    stored.clone(),
                    text("h"),
                ]]),
            );
            let request = rows_request(&["body"], vec![shown(0, 1, vec![shown_cell.clone()])]);

            let rows = changed(db, |_| {}, &request, CellLimit::NONE).await;

            let expected: Vec<usize> = if stored == Cell::Null {
                vec![0]
            } else {
                vec![]
            };
            assert_eq!(rows[0].changed, expected, "{stored:?}");
        }
    }

    #[tokio::test]
    async fn bytes_are_compared_by_length_and_preview() {
        let bytes = |len: usize, hex: &str| Cell::Bytes {
            len,
            hex_preview: hex.into(),
        };
        let cases = [
            ("same", bytes(20, "0a0b"), bytes(20, "0a0b"), false),
            ("longer", bytes(20, "0a0b"), bytes(21, "0a0b"), true),
            ("other preview", bytes(20, "0a0b"), bytes(20, "0a0c"), true),
            ("text now", bytes(2, "0a0b"), text("x"), true),
        ];
        for (what, old, new, differs) in cases {
            let db = FakeDatabase::new().on(
                "FROM",
                canned(vec![vec![Cell::Number("1".into()), new, text("h")]]),
            );
            let request = rows_request(&["blob"], vec![shown(0, 1, vec![old])]);

            let rows = changed(db, |_| {}, &request, CellLimit::NONE).await;

            assert_eq!(rows[0].changed.is_empty(), !differs, "{what}");
        }
    }

    #[tokio::test]
    async fn a_row_whose_checksum_moved_but_whose_shown_cells_match_has_no_changed_columns() {
        let db = FakeDatabase::new().on(
            "FROM",
            canned(vec![vec![
                Cell::Number("1".into()),
                Cell::Bytes {
                    len: 40,
                    hex_preview: "00".repeat(16),
                },
                text("new-hash"),
            ]]),
        );
        let request = rows_request(
            &["blob"],
            vec![shown(
                0,
                1,
                vec![Cell::Bytes {
                    len: 40,
                    hex_preview: "00".repeat(16),
                }],
            )],
        );

        let rows = changed(db, |_| {}, &request, CellLimit::NONE).await;

        assert_eq!(rows[0].hash, "new-hash");
        assert!(rows[0].changed.is_empty());
    }

    #[tokio::test]
    async fn returned_text_cells_are_cut_to_the_cell_limit_after_the_comparison() {
        let db = FakeDatabase::new().on(
            "FROM",
            canned(vec![vec![
                Cell::Number("1".into()),
                text("abcdefgh"),
                text("abc"),
                Cell::Number("123456789".into()),
                text("h"),
            ]]),
        );
        let request = rows_request(
            &["long", "short", "number"],
            vec![shown(
                0,
                1,
                vec![
                    text("abcdefgh"),
                    text("abc"),
                    Cell::Number("123456789".into()),
                ],
            )],
        );

        let rows = changed(db, |_| {}, &request, CellLimit(5)).await;

        assert_eq!(
            rows[0].cells,
            [
                Cell::LongText {
                    len: 8,
                    preview: "abcde".into()
                },
                text("abc"),
                Cell::Number("123456789".into()),
            ]
        );
        assert!(rows[0].changed.is_empty());
    }

    #[test]
    fn checked_row_serializes_with_camel_case_fields() {
        let row = CheckedRow {
            row: 2,
            hash: "h".into(),
            cells: vec![Cell::Null],
            changed: vec![0],
        };

        assert_eq!(
            serde_json::to_value(&row).unwrap(),
            serde_json::json!({
                "row": 2,
                "hash": "h",
                "cells": [{"t": "null"}],
                "changed": [0]
            })
        );
    }

    #[test]
    fn requests_deserialize_from_camel_case() {
        let request: TableCheckRowsRequest = serde_json::from_value(serde_json::json!({
            "database": "wp",
            "table": "users",
            "columns": ["name"],
            "resultId": 7,
            "rows": [{
                "row": 0,
                "key": [{"column": "id", "value": "1"}],
                "cells": [{"t": "text", "v": "a"}]
            }]
        }))
        .unwrap();

        assert_eq!(
            request,
            rows_request(&["name"], vec![shown(0, 1, vec![text("a")])])
        );
    }

    #[tokio::test]
    #[ignore]
    async fn server_hashes_tell_equal_rows_from_null_and_edited_ones() {
        for (host, port) in servers() {
            let server = format!("{host}:{port}");
            let db = connect_to(&host, port, "databee_test").await;
            let session = Session::new(
                "it".into(),
                spec(&host, port, "databee_test"),
                "it".into(),
                Box::new(db),
            );
            session
                .db
                .execute(
                    "DROP TABLE IF EXISTS table_check_probe; \
                     CREATE TABLE table_check_probe \
                         (id int PRIMARY KEY, a varchar(20) NULL, b varchar(20) NULL); \
                     INSERT INTO table_check_probe VALUES \
                         (1,'x','y'), (2,'x','y'), (3,'','y'), (4,NULL,'y')",
                    10,
                    CellLimit::NONE,
                    TIMEOUT,
                )
                .await
                .unwrap_or_else(|e| panic!("{server}: {e:?}"));
            let request = TableCheckRequest {
                database: "databee_test".into(),
                table: "table_check_probe".into(),
                columns: strings(&["a", "b"]),
                keys: vec![id_key(1), id_key(2), id_key(3), id_key(4), id_key(9)],
            };

            let before = check(&session, &request, TIMEOUT)
                .await
                .unwrap_or_else(|e| panic!("{server}: {e:?}"))
                .hashes;

            assert_eq!(before[0], before[1], "{server}: equal rows");
            assert!(before[0].is_some(), "{server}");
            assert!(before[2].is_some() && before[3].is_some(), "{server}");
            assert_ne!(before[2], before[3], "{server}: '' and NULL");
            assert_eq!(before[4], None, "{server}: missing row");

            session
                .db
                .execute(
                    "UPDATE table_check_probe SET a = 'z' WHERE id = 1",
                    10,
                    CellLimit::NONE,
                    TIMEOUT,
                )
                .await
                .unwrap();
            let after = check(&session, &request, TIMEOUT).await.unwrap().hashes;
            let rows = changed_rows(
                &session,
                &TableCheckRowsRequest {
                    database: "databee_test".into(),
                    table: "table_check_probe".into(),
                    columns: strings(&["a", "b"]),
                    result_id: 1,
                    rows: vec![shown(0, 1, vec![text("x"), text("y")])],
                },
                CellLimit::NONE,
                TIMEOUT,
            )
            .await
            .unwrap();
            session
                .db
                .execute("DROP TABLE table_check_probe", 10, CellLimit::NONE, TIMEOUT)
                .await
                .unwrap();

            assert_ne!(before[0], after[0], "{server}: updated row");
            assert_eq!(before[1], after[1], "{server}: untouched row");
            assert_eq!(rows.len(), 1, "{server}");
            assert_eq!(rows[0].changed, [0], "{server}");
            assert_eq!(Some(rows[0].hash.clone()), after[0], "{server}");
            assert_eq!(rows[0].cells, [text("z"), text("y")], "{server}");
        }
    }
}
