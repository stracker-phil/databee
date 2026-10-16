//! Runs against the Docker fixtures in `dev/docker-compose.yml` (`bun run test:db`).
//! Override servers with DATABEE_TEST_SERVERS="host:port,host:port" (user root/root).

use std::sync::Arc;
use std::time::Duration;

use databee_lib::connection::{ConnectionSpec, Secret};
use databee_lib::db::Database;
use databee_lib::db::mysql::MySqlSession;
use databee_lib::db::{BrowseRequest, Cell};

const TIMEOUT: Duration = Duration::from_secs(10);

fn servers() -> Vec<(String, u16)> {
    std::env::var("DATABEE_TEST_SERVERS")
        .unwrap_or_else(|_| "127.0.0.1:33306,127.0.0.1:33307".into())
        .split(',')
        .map(|s| {
            let (h, p) = s.rsplit_once(':').unwrap();
            (h.to_string(), p.parse().unwrap())
        })
        .collect()
}

fn spec(host: &str, port: u16) -> ConnectionSpec {
    ConnectionSpec {
        name: None,
        host: host.into(),
        port,
        socket: None,
        user: "root".into(),
        database: Some("databee_test".into()),
    }
}

async fn sessions() -> Vec<MySqlSession> {
    let mut out = Vec::new();
    for (host, port) in servers() {
        let s = MySqlSession::connect(
            &spec(&host, port),
            Some(&Secret::new("root")),
            TIMEOUT,
            TIMEOUT,
        )
        .await
        .unwrap_or_else(|e| panic!("{host}:{port}: {e}"));
        out.push(s);
    }
    out
}

fn text(c: &Cell) -> &str {
    match c {
        Cell::Number(s) | Cell::Text(s) | Cell::Temporal(s) | Cell::Json(s) => s,
        other => panic!("not a text-like cell: {other:?}"),
    }
}

#[tokio::test]
#[ignore]
async fn connects_and_reports_server_version_and_database() {
    for s in sessions().await {
        assert!(!s.server_version().is_empty());
        assert_eq!(s.current_database().as_deref(), Some("databee_test"));
    }
}

#[tokio::test]
#[ignore]
async fn fresh_session_uses_backslash_escapes_under_the_default_sql_mode() {
    for s in sessions().await {
        assert!(s.dialect().backslash_escapes, "{}", s.server_version());
    }
}

#[tokio::test]
#[ignore]
async fn wrong_password_is_an_auth_error() {
    for (host, port) in servers() {
        let err = MySqlSession::connect(
            &spec(&host, port),
            Some(&Secret::new("nope")),
            TIMEOUT,
            TIMEOUT,
        )
        .await
        .err()
        .unwrap();
        assert_eq!(err.kind(), "auth_failed", "{host}:{port}: {err:?}");
    }
}

#[tokio::test]
#[ignore]
async fn unreachable_server_is_a_connection_error() {
    let err = MySqlSession::connect(&spec("127.0.0.1", 1), None, TIMEOUT, TIMEOUT)
        .await
        .err()
        .unwrap();
    assert_eq!(err.kind(), "connection");
}

#[tokio::test]
#[ignore]
async fn lists_databases_tables_and_views() {
    for s in sessions().await {
        let dbs = s.databases().await.unwrap();
        assert!(dbs.iter().any(|d| d.name == "databee_test" && !d.is_system));
        assert!(dbs.iter().any(|d| d.name == "mysql" && d.is_system));

        let tables = s.tables("databee_test").await.unwrap();
        let names: Vec<_> = tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["published", "types", "wp_posts"]);
        assert!(tables[0].is_view);
        assert_eq!(tables[2].comment, "Posts");
    }
}

#[tokio::test]
#[ignore]
async fn describes_columns_indexes_and_ddl() {
    for s in sessions().await {
        let st = s.structure("databee_test", "wp_posts").await.unwrap();
        assert_eq!(st.columns[0].name, "ID");
        assert_eq!(st.columns[0].key, "PRI");
        assert!(st.columns[0].extra.contains("auto_increment"));
        assert_eq!(st.indexes[0].name, "PRIMARY");
        assert!(st.indexes[0].unique);
        assert_eq!(
            st.indexes[1].columns,
            ["post_type", "post_status", "post_date", "ID"]
        );
        assert!(
            st.ddl
                .as_deref()
                .unwrap()
                .starts_with("CREATE TABLE `wp_posts`")
        );
    }
}

#[tokio::test]
#[ignore]
async fn decodes_each_type_family() {
    for s in sessions().await {
        let r = s
            .execute("SELECT * FROM types ORDER BY id", 100, TIMEOUT)
            .await
            .unwrap();
        let row = &r.results[0].rows[0];
        assert_eq!(text(&row[1]), "18446744073709551615");
        assert_eq!(text(&row[2]), "49.90");
        assert_eq!(text(&row[3]), "0.5");
        assert_eq!(text(&row[4]), "1");
        assert_eq!(row[5], Cell::Temporal("2026-10-15 20:00:00".into()));
        assert_eq!(text(&row[6]), "0000-00-00 00:00:00");
        assert!(text(&row[7]).contains("\"a\""));
        assert_eq!(
            row[8],
            Cell::Bytes {
                len: 3,
                hex_preview: "00ff10".into()
            }
        );
        assert!(matches!(row[9], Cell::Bytes { len: 1, .. }));
        assert_eq!(row[10], Cell::Text("bin text".into()));
        assert!(r.results[0].rows[1][1..].iter().all(|c| *c == Cell::Null));
    }
}

#[tokio::test]
#[ignore]
async fn row_limit_truncates_but_counts_all_rows() {
    let sql = "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 50) SELECT i FROM n";
    for s in sessions().await {
        let set = &s.execute(sql, 10, TIMEOUT).await.unwrap().results[0];
        assert_eq!(set.rows.len(), 10);
        assert_eq!(set.total_rows, 50);
        assert!(set.truncated);
    }
}

#[tokio::test]
#[ignore]
async fn empty_result_still_has_columns() {
    for s in sessions().await {
        let set = &s
            .execute(
                "SELECT ID, post_title FROM wp_posts WHERE 1 = 0",
                10,
                TIMEOUT,
            )
            .await
            .unwrap()
            .results[0];
        assert!(set.returns_rows);
        assert!(set.rows.is_empty());
        let names: Vec<_> = set.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["ID", "post_title"]);
    }
}

#[tokio::test]
#[ignore]
async fn scripts_return_one_result_per_statement_with_affected_rows() {
    for s in sessions().await {
        let r = s
            .execute(
                "CREATE TEMPORARY TABLE tmp (id int AUTO_INCREMENT PRIMARY KEY, v int); \
                 INSERT INTO tmp (v) VALUES (1), (2), (3); \
                 UPDATE tmp SET v = 0 WHERE v > 1; \
                 SELECT COUNT(*) FROM tmp",
                10,
                TIMEOUT,
            )
            .await
            .unwrap();
        assert_eq!(r.results.len(), 4);
        assert_eq!(r.results[1].affected_rows, 3);
        assert_eq!(r.results[1].last_insert_id, Some(1));
        assert_eq!(r.results[2].affected_rows, 2);
        assert!(!r.results[2].returns_rows);
        assert_eq!(text(&r.results[3].rows[0][0]), "3");
    }
}

#[tokio::test]
#[ignore]
async fn sql_errors_carry_the_server_code() {
    for s in sessions().await {
        let err = s
            .execute("SELECT * FROM missing_table", 10, TIMEOUT)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), "database");
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "1146");
    }
}

#[tokio::test]
#[ignore]
async fn use_changes_the_current_database_and_session_state_persists() {
    for s in sessions().await {
        let r = s.execute("USE mysql", 10, TIMEOUT).await.unwrap();
        assert_eq!(r.database.as_deref(), Some("mysql"));
        s.execute("SET @x = 42", 10, TIMEOUT).await.unwrap();
        let r = s.execute("SELECT @x", 10, TIMEOUT).await.unwrap();
        assert_eq!(text(&r.results[0].rows[0][0]), "42");
        s.use_database("databee_test").await.unwrap();
        assert_eq!(s.current_database().as_deref(), Some("databee_test"));
    }
}

#[tokio::test]
#[ignore]
async fn timeout_aborts_and_session_recovers() {
    for s in sessions().await {
        let err = s
            .execute("SELECT SLEEP(5)", 10, Duration::from_secs(1))
            .await
            .unwrap_err();
        assert_eq!(err.kind(), "timeout");
        let r = s.execute("SELECT DATABASE()", 10, TIMEOUT).await.unwrap();
        assert_eq!(text(&r.results[0].rows[0][0]), "databee_test");
    }
}

#[tokio::test]
#[ignore]
async fn cancel_interrupts_a_running_query() {
    for s in sessions().await {
        let s = Arc::new(s);
        let runner = {
            let s = s.clone();
            tokio::spawn(async move { s.execute("SELECT SLEEP(10)", 10, TIMEOUT).await })
        };
        tokio::time::sleep(Duration::from_millis(500)).await;
        s.cancel().await.unwrap();
        // MySQL reports an interrupted SLEEP as a result row (1); MariaDB as error 1317.
        match runner.await.unwrap() {
            Err(e) => assert_eq!(e.kind(), "cancelled"),
            Ok(r) => assert_eq!(text(&r.results[0].rows[0][0]), "1"),
        }
        assert!(s.execute("SELECT 1", 10, TIMEOUT).await.is_ok());
    }
}

async fn connect(host: &str, port: u16) -> MySqlSession {
    MySqlSession::connect(
        &spec(host, port),
        Some(&Secret::new("root")),
        TIMEOUT,
        TIMEOUT,
    )
    .await
    .unwrap_or_else(|e| panic!("{host}:{port}: {e}"))
}

#[tokio::test]
#[ignore]
async fn with_delete_keeps_its_affected_rows() {
    for s in sessions().await {
        s.execute("DROP TABLE IF EXISTS cte_delete", 10, TIMEOUT)
            .await
            .unwrap();
        if s.server_version().contains("MariaDB") {
            continue;
        }
        let r = s
            .execute(
                "DROP TABLE IF EXISTS cte_delete; \
                 CREATE TABLE cte_delete (id int PRIMARY KEY); \
                 INSERT INTO cte_delete VALUES (1), (2), (3); \
                 WITH c AS (SELECT 1) DELETE FROM cte_delete; \
                 DROP TABLE cte_delete",
                10,
                TIMEOUT,
            )
            .await
            .unwrap();
        let delete = &r.results[3];
        assert_eq!(delete.affected_rows, 3);
        assert!(!delete.returns_rows);
        assert!(delete.columns.is_empty());
    }
}

#[tokio::test]
#[ignore]
async fn structure_of_a_created_table_includes_its_ddl() {
    for s in sessions().await {
        s.execute(
            "DROP TABLE IF EXISTS ddl_probe; CREATE TABLE ddl_probe (id int PRIMARY KEY)",
            10,
            TIMEOUT,
        )
        .await
        .unwrap();
        let st = s.structure("databee_test", "ddl_probe").await;
        s.execute("DROP TABLE ddl_probe", 10, TIMEOUT)
            .await
            .unwrap();
        let ddl = st.unwrap().ddl;
        assert!(
            ddl.as_deref()
                .is_some_and(|d| d.starts_with("CREATE TABLE")),
            "{ddl:?}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn cancel_reaches_the_editor_while_every_pool_connection_is_busy() {
    for (host, port) in servers() {
        let s = Arc::new(connect(&host, port).await);
        let busy: Vec<_> = (0..3)
            .map(|_| {
                let s = s.clone();
                tokio::spawn(async move { s.fetch("SELECT SLEEP(5)", TIMEOUT).await })
            })
            .collect();
        let runner = {
            let s = s.clone();
            tokio::spawn(async move { s.execute("SELECT SLEEP(10)", 10, TIMEOUT).await })
        };
        tokio::time::sleep(Duration::from_millis(1000)).await;

        let started = std::time::Instant::now();
        s.cancel()
            .await
            .unwrap_or_else(|e| panic!("{host}:{port}: {e:?}"));
        match runner.await.unwrap() {
            Err(e) => assert_eq!(e.kind(), "cancelled", "{host}:{port}"),
            Ok(r) => assert_eq!(text(&r.results[0].rows[0][0]), "1", "{host}:{port}"),
        }
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "{host}:{port}: cancel took {:?}",
            started.elapsed()
        );
        for task in busy {
            task.await.unwrap().unwrap();
        }
    }
}

#[tokio::test]
#[ignore]
async fn killed_editor_connection_fails_once_then_reconnects() {
    for (host, port) in servers() {
        let s = connect(&host, port).await;
        let other = connect(&host, port).await;
        let id = s
            .execute("SELECT CONNECTION_ID()", 10, TIMEOUT)
            .await
            .unwrap();
        let id = text(&id.results[0].rows[0][0]).to_string();
        other
            .execute(&format!("KILL {id}"), 10, TIMEOUT)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;

        let err = s.execute("SELECT 1", 10, TIMEOUT).await.unwrap_err();
        assert_eq!(err.kind(), "connection", "{host}:{port}: {err:?}");
        let r = s
            .execute("SELECT 1", 10, TIMEOUT)
            .await
            .unwrap_or_else(|e| panic!("{host}:{port}: {e:?}"));
        assert_eq!(text(&r.results[0].rows[0][0]), "1", "{host}:{port}");
    }
}

#[tokio::test]
#[ignore]
async fn browse_pages_with_order_and_filter() {
    for s in sessions().await {
        let req = BrowseRequest {
            database: "databee_test".into(),
            table: "wp_posts".into(),
            limit: 2,
            order_by: Some("ID".into()),
            descending: true,
            ..Default::default()
        };
        let first = s.browse(&req, TIMEOUT).await.unwrap();
        assert!(first.has_more);
        assert_eq!(text(&first.result.rows[0][0]), "3");

        let second = s
            .browse(
                &BrowseRequest {
                    offset: 2,
                    ..req.clone()
                },
                TIMEOUT,
            )
            .await
            .unwrap();
        assert!(!second.has_more);
        assert_eq!(second.result.rows.len(), 1);

        let filtered = s
            .browse(
                &BrowseRequest {
                    filter: Some("post_type = 'page'".into()),
                    ..req
                },
                TIMEOUT,
            )
            .await
            .unwrap();
        assert_eq!(filtered.result.rows.len(), 1);
    }
}

#[tokio::test]
#[ignore]
async fn completion_maps_tables_to_columns() {
    for s in sessions().await {
        let map = s.completion("databee_test").await.unwrap();
        assert_eq!(map["published"], ["ID", "post_title"]);
    }
}
