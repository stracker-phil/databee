//! Row edits planned by the core (`db::changes`) against the Docker fixtures (`bun run test:db`).
//! Every test that writes owns its rows in `wp_write` (see `dev/fixture.sql`), so tests never
//! share one.

use std::time::Duration;

use databee_lib::db::changes::{ChangeRequest, Plan, plan};
use databee_lib::db::mysql::MySqlSession;
use databee_lib::db::{Cell, Database};
use databee_lib::decode::edit::{NewValue, edit_serialized};
use databee_lib::decode::php;

mod common;

use common::sessions;

const TIMEOUT: Duration = Duration::from_secs(10);

fn text(cell: &Cell) -> Option<String> {
    cell.as_text().map(str::to_string)
}

async fn scalar(db: &MySqlSession, sql: &str) -> Option<String> {
    let rs = db.fetch(sql, TIMEOUT).await.unwrap();
    rs.rows.first().and_then(|r| text(&r[0]))
}

async fn apply(db: &MySqlSession, statements: &[String]) {
    db.apply(statements, TIMEOUT).await.unwrap();
}

async fn row_plan(db: &MySqlSession, request: ChangeRequest) -> Plan {
    plan(db, &request, TIMEOUT).await.unwrap()
}

/// The first row of `sql`, NULL cells as `None`.
async fn first_row(db: &MySqlSession, sql: &str) -> Vec<Option<String>> {
    let rs = db.fetch(sql, TIMEOUT).await.unwrap();
    rs.rows[0].iter().map(text).collect()
}

fn set_cell(
    table: &str,
    key_column: &str,
    key: &str,
    column: &str,
    value: Option<&str>,
) -> ChangeRequest {
    ChangeRequest::SetCell {
        database: "wp_write".into(),
        table: table.into(),
        key_column: key_column.into(),
        key: key.into(),
        column: column.into(),
        value: value.map(String::from),
    }
}

#[tokio::test]
#[ignore]
async fn editing_a_serialized_value_by_path_keeps_it_parseable_and_undo_restores_it() {
    for db in sessions("wp_write").await {
        let read =
            "SELECT option_value FROM wp_write.wp_options WHERE option_name = 'zz_serialized_edit'";
        let raw = scalar(&db, read).await.unwrap();
        let edited = edit_serialized(&raw, &[1], &NewValue::String("PayPal ✓".into())).unwrap();
        let change = row_plan(
            &db,
            set_cell(
                "wp_options",
                "option_name",
                "zz_serialized_edit",
                "option_value",
                Some(&edited),
            ),
        )
        .await;
        apply(&db, &change.statements).await;
        let stored = scalar(&db, read).await.unwrap();
        assert_eq!(stored, edited);
        assert!(php::parse(stored.as_bytes()).is_ok(), "{stored}");
        apply(&db, &change.undo).await;
        assert_eq!(scalar(&db, read).await.unwrap(), raw);
    }
}

#[tokio::test]
#[ignore]
async fn deleting_a_row_with_a_null_cell_restores_it_exactly() {
    for db in sessions("wp_write").await {
        let read = "SELECT meta_id, post_id, meta_key, meta_value FROM wp_write.wp_postmeta WHERE meta_id = 9301";
        let before = first_row(&db, read).await;
        assert_eq!(before[3], None, "the fixture row has a NULL meta_value");
        let change = row_plan(
            &db,
            ChangeRequest::DeleteRow {
                database: "wp_write".into(),
                table: "wp_postmeta".into(),
                key_column: "meta_id".into(),
                key: "9301".into(),
            },
        )
        .await;
        apply(&db, &change.statements).await;
        assert_eq!(
            scalar(
                &db,
                "SELECT COUNT(*) FROM wp_write.wp_postmeta WHERE meta_id = 9301"
            )
            .await
            .as_deref(),
            Some("0")
        );
        apply(&db, &change.undo).await;
        assert_eq!(first_row(&db, read).await, before);
    }
}

#[tokio::test]
#[ignore]
async fn duplicating_a_row_inserts_a_copy_with_a_new_auto_increment_id() {
    for db in sessions("wp_write").await {
        let copies = "SELECT meta_id, post_id, meta_value FROM wp_write.wp_postmeta WHERE meta_key = 'zz_duplicate_me' ORDER BY meta_id";
        let change = row_plan(
            &db,
            ChangeRequest::DuplicateRow {
                database: "wp_write".into(),
                table: "wp_postmeta".into(),
                key_column: "meta_id".into(),
                key: "9302".into(),
            },
        )
        .await;
        assert!(change.undo.is_empty());
        apply(&db, &change.statements).await;
        let rs = db.fetch(copies, TIMEOUT).await.unwrap();
        let rows: Vec<(Option<String>, Option<String>)> =
            rs.rows.iter().map(|r| (text(&r[1]), text(&r[2]))).collect();
        let ids: Vec<Option<String>> = rs.rows.iter().map(|r| text(&r[0])).collect();
        apply(
            &db,
            &["DELETE FROM wp_write.wp_postmeta WHERE meta_key = 'zz_duplicate_me' AND meta_id <> 9302".into()],
        )
        .await;
        let copy = (Some("123".to_string()), Some("v".to_string()));
        assert_eq!(rows, [copy.clone(), copy]);
        assert_eq!(ids[0].as_deref(), Some("9302"));
        assert_ne!(ids[1].as_deref(), Some("9302"));
    }
}

#[tokio::test]
#[ignore]
async fn inserting_a_row_writes_null_for_a_missing_value() {
    for db in sessions("wp_write").await {
        let change = row_plan(
            &db,
            ChangeRequest::InsertRow {
                database: "wp_write".into(),
                table: "wp_postmeta".into(),
                values: vec![
                    ("post_id".into(), Some("123".into())),
                    ("meta_key".into(), Some("zz_inserted".into())),
                    ("meta_value".into(), None),
                ],
            },
        )
        .await;
        assert!(change.undo.is_empty());
        apply(&db, &change.statements).await;
        let stored = first_row(
            &db,
            "SELECT post_id, meta_key, meta_value FROM wp_write.wp_postmeta WHERE meta_key = 'zz_inserted'",
        )
        .await;
        apply(
            &db,
            &["DELETE FROM wp_write.wp_postmeta WHERE meta_key = 'zz_inserted'".into()],
        )
        .await;
        assert_eq!(
            stored,
            [
                Some("123".to_string()),
                Some("zz_inserted".to_string()),
                None
            ]
        );
    }
}

#[tokio::test]
#[ignore]
async fn undo_restores_an_empty_string_instead_of_null() {
    for db in sessions("wp_write").await {
        let read = "SELECT post_name FROM wp_write.wp_posts WHERE ID = 9601";
        assert_eq!(scalar(&db, read).await.as_deref(), Some(""));
        let change = row_plan(
            &db,
            set_cell("wp_posts", "ID", "9601", "post_name", Some("x")),
        )
        .await;
        apply(&db, &change.statements).await;
        assert_eq!(scalar(&db, read).await.as_deref(), Some("x"));
        apply(&db, &change.undo).await;
        assert_eq!(
            scalar(&db, read).await.as_deref(),
            Some(""),
            "restored as '' and not NULL"
        );
    }
}

#[tokio::test]
#[ignore]
async fn undo_restores_a_null_cell_to_null() {
    for db in sessions("wp_write").await {
        let read = "SELECT email FROM wp_write.wp_wc_order_addresses WHERE id = 9602";
        assert_eq!(scalar(&db, read).await, None);
        let change = row_plan(
            &db,
            set_cell(
                "wp_wc_order_addresses",
                "id",
                "9602",
                "email",
                Some("ship@shop.test"),
            ),
        )
        .await;
        apply(&db, &change.statements).await;
        assert_eq!(scalar(&db, read).await.as_deref(), Some("ship@shop.test"));
        apply(&db, &change.undo).await;
        assert_eq!(scalar(&db, read).await, None, "restored as NULL");
    }
}

#[tokio::test]
#[ignore]
async fn editing_a_binary_cell_has_no_undo() {
    for db in sessions("databee_test").await {
        let change = row_plan(
            &db,
            ChangeRequest::SetCell {
                database: "databee_test".into(),
                table: "types".into(),
                key_column: "id".into(),
                key: "1".into(),
                column: "blob_data".into(),
                value: Some("x".into()),
            },
        )
        .await;
        assert!(change.undo.is_empty(), "{:?}", change.undo);
        assert!(
            change.description.contains("no undo"),
            "{}",
            change.description
        );
    }
}

#[tokio::test]
#[ignore]
async fn undo_of_a_key_column_change_finds_the_row_by_its_new_key() {
    for db in sessions("wp_write").await {
        let count = |name: &'static str| {
            let db = &db;
            async move {
                scalar(
                    db,
                    &format!(
                        "SELECT COUNT(*) FROM wp_write.wp_options WHERE option_name = '{name}'"
                    ),
                )
                .await
            }
        };
        let change = row_plan(
            &db,
            set_cell(
                "wp_options",
                "option_name",
                "zz_rename_me",
                "option_name",
                Some("zz_renamed"),
            ),
        )
        .await;
        apply(&db, &change.statements).await;
        assert_eq!(count("zz_renamed").await.as_deref(), Some("1"));
        assert_eq!(count("zz_rename_me").await.as_deref(), Some("0"));
        apply(&db, &change.undo).await;
        assert_eq!(count("zz_rename_me").await.as_deref(), Some("1"));
        assert_eq!(count("zz_renamed").await.as_deref(), Some("0"));
    }
}
