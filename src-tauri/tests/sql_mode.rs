//! Writes under `NO_BACKSLASH_ESCAPES`. The dialect is read when a session connects, so the
//! server's global `sql_mode` is switched for the new session and put back afterwards. This is
//! its own test binary so no other test connects while the global mode is changed.

use std::time::Duration;

use databee_lib::db::changes::{self, ChangeRequest};
use databee_lib::db::{CellLimit, Database};
use databee_lib::error::AppResult;

mod common;

use common::{connect_to, servers};

const TIMEOUT: Duration = Duration::from_secs(10);
const VALUE: &str = r"it's\x";

async fn scalar(db: &dyn Database, sql: &str) -> String {
    let rows = db.fetch(sql, TIMEOUT).await.unwrap();
    rows.rows[0][0].as_text().unwrap().to_string()
}

struct Outcome {
    backslash_escapes: bool,
    statements: Vec<String>,
    undo: Vec<String>,
    applied: String,
    restored: String,
}

async fn edit_title(host: &str, port: u16) -> AppResult<Outcome> {
    let db = connect_to(host, port, "wp_write").await;
    let read = "SELECT post_title FROM wp_posts WHERE ID = 9601";

    let change = ChangeRequest::SetCell {
        database: "wp_write".into(),
        table: "wp_posts".into(),
        key_column: "ID".into(),
        key: "9601".into(),
        column: "post_title".into(),
        value: Some(VALUE.into()),
    };
    let plan = changes::plan(&db, &change, TIMEOUT).await?;
    db.apply(&plan.statements, TIMEOUT).await?;
    let applied = scalar(&db, read).await;
    db.apply(&plan.undo, TIMEOUT).await?;
    let restored = scalar(&db, read).await;

    let outcome = Outcome {
        backslash_escapes: db.dialect().backslash_escapes,
        statements: plan.statements,
        undo: plan.undo,
        applied,
        restored,
    };
    db.close().await;
    Ok(outcome)
}

#[tokio::test]
#[ignore]
async fn set_cell_with_quote_and_backslash_round_trips_under_no_backslash_escapes() {
    for (host, port) in servers() {
        let admin = connect_to(&host, port, "wp_write").await;
        let previous = scalar(&admin, "SELECT @@GLOBAL.sql_mode").await;
        let original = scalar(
            &admin,
            "SELECT post_title FROM wp_write.wp_posts WHERE ID = 9601",
        )
        .await;
        let mode = if previous.is_empty() {
            "NO_BACKSLASH_ESCAPES".to_string()
        } else {
            format!("{previous},NO_BACKSLASH_ESCAPES")
        };
        let set = |mode: &str| format!("SET GLOBAL sql_mode = '{mode}'");
        admin
            .execute(&set(&mode), 1, CellLimit::NONE, TIMEOUT)
            .await
            .unwrap();

        let outcome = edit_title(&host, port).await;

        admin
            .execute(&set(&previous), 1, CellLimit::NONE, TIMEOUT)
            .await
            .unwrap();
        let outcome = outcome.unwrap_or_else(|e| panic!("{host}:{port}: {e:?}"));

        assert!(!outcome.backslash_escapes, "{host}:{port}");
        assert_eq!(
            outcome.statements,
            ["UPDATE `wp_write`.`wp_posts` SET `post_title` = 'it''s\\x' WHERE `ID` = '9601'"],
            "{host}:{port}"
        );
        assert_eq!(
            outcome.undo,
            [format!(
                "UPDATE `wp_write`.`wp_posts` SET `post_title` = '{}' WHERE `ID` = '9601'",
                original.replace('\'', "''")
            )],
            "{host}:{port}"
        );
        assert_eq!(outcome.applied, VALUE, "{host}:{port}");
        assert_eq!(outcome.restored, original, "{host}:{port}");
        assert_eq!(
            scalar(&admin, "SELECT @@GLOBAL.sql_mode").await,
            previous,
            "{host}:{port}: global sql_mode is put back"
        );
    }
}
