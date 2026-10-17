//! Runs against the Docker fixtures in `dev/docker-compose.yml` (`bun run test:db`).
//! Override servers with DATABEE_TEST_SERVERS="host:port,host:port" (user root/root).

use std::sync::Arc;
use std::time::Duration;

use databee_lib::connection::Secret;
use databee_lib::db::Database;
use databee_lib::db::cell_cache::CellCache;
use databee_lib::db::mysql::MySqlSession;
use databee_lib::db::{BrowseRequest, Cell, CellLimit, FilterKind, ForeignKey};
use databee_lib::error::ErrorKind;

mod common;

use common::{connect_to, servers, sessions, spec};

const DATABASE: &str = "databee_test";
const TIMEOUT: Duration = Duration::from_secs(10);
const CELLS: CellLimit = CellLimit::NONE;

fn text(c: &Cell) -> &str {
    match c {
        Cell::Number(s) | Cell::Text(s) | Cell::Temporal(s) | Cell::Json(s) => s,
        other => panic!("not a text-like cell: {other:?}"),
    }
}

#[tokio::test]
#[ignore]
async fn connects_and_reports_server_version_and_database() {
    for s in sessions(DATABASE).await {
        assert!(!s.server_version().is_empty());
        assert_eq!(s.current_database().as_deref(), Some("databee_test"));
    }
}

#[tokio::test]
#[ignore]
async fn fresh_session_uses_backslash_escapes_under_the_default_sql_mode() {
    for s in sessions(DATABASE).await {
        assert!(s.dialect().backslash_escapes, "{}", s.server_version());
    }
}

#[tokio::test]
#[ignore]
async fn wrong_password_is_an_auth_error() {
    for (host, port) in servers() {
        let err = MySqlSession::connect(
            &spec(&host, port, DATABASE),
            Some(&Secret::new("nope")),
            TIMEOUT,
            TIMEOUT,
        )
        .await
        .err()
        .unwrap();
        assert_eq!(err.kind(), ErrorKind::AuthFailed, "{host}:{port}: {err:?}");
    }
}

#[tokio::test]
#[ignore]
async fn unreachable_server_is_a_connection_error() {
    let err = MySqlSession::connect(&spec("127.0.0.1", 1, DATABASE), None, TIMEOUT, TIMEOUT)
        .await
        .err()
        .unwrap();
    assert_eq!(err.kind(), ErrorKind::Connection);
}

#[tokio::test]
#[ignore]
async fn lists_databases_tables_and_views() {
    for s in sessions(DATABASE).await {
        let dbs = s.databases().await.unwrap();
        assert!(dbs.iter().any(|d| d.name == "databee_test" && !d.is_system));
        assert!(dbs.iter().any(|d| d.name == "mysql" && d.is_system));

        // Other tests create short-lived `*_probe` tables in this database.
        let mut tables = s.tables("databee_test").await.unwrap();
        tables.retain(|t| !t.name.ends_with("_probe"));
        let names: Vec<_> = tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            ["authors", "books", "published", "types", "wp_posts"]
        );
        assert!(tables[2].is_view);
        assert_eq!(tables[4].comment, "Posts");
    }
}

fn books_author() -> ForeignKey {
    ForeignKey {
        name: "books_author".into(),
        columns: vec!["author_id".into()],
        ref_table: "authors".into(),
        ref_columns: vec!["id".into()],
    }
}

#[tokio::test]
#[ignore]
async fn structure_lists_declared_foreign_keys() {
    for s in sessions(DATABASE).await {
        let books = s.structure(DATABASE, "books").await.unwrap();
        let authors = s.structure(DATABASE, "authors").await.unwrap();

        assert_eq!(books.foreign_keys, [books_author()]);
        assert!(authors.foreign_keys.is_empty());
    }
}

#[tokio::test]
#[ignore]
async fn bulk_keys_include_foreign_keys() {
    for s in sessions(DATABASE).await {
        let keys = s.keys(DATABASE).await.unwrap();

        assert_eq!(keys["books"].foreign_keys, [books_author()]);
        assert_eq!(keys["books"].primary_key, ["id"]);
        assert!(keys["authors"].foreign_keys.is_empty());
    }
}

#[tokio::test]
#[ignore]
async fn bulk_keys_give_every_wordpress_table_a_primary_key() {
    for s in sessions("wp_test").await {
        let keys = s.keys("wp_test").await.unwrap();
        let tables = s.tables("wp_test").await.unwrap();

        for table in tables.iter().filter(|t| !t.is_view) {
            let primary_key = keys.get(&table.name).map(|k| &k.primary_key);
            assert!(
                primary_key.is_some_and(|pk| !pk.is_empty()),
                "{}: {}",
                s.server_version(),
                table.name
            );
        }
    }
}

#[tokio::test]
#[ignore]
async fn describes_columns_indexes_and_ddl() {
    for s in sessions(DATABASE).await {
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
    for s in sessions(DATABASE).await {
        let r = s
            .execute("SELECT * FROM types ORDER BY id", 100, CELLS, TIMEOUT)
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
    for s in sessions(DATABASE).await {
        let set = &s.execute(sql, 10, CELLS, TIMEOUT).await.unwrap().results[0];
        assert_eq!(set.rows.len(), 10);
        assert_eq!(set.total_rows, 50);
        assert!(set.truncated);
    }
}

#[tokio::test]
#[ignore]
async fn empty_result_still_has_columns() {
    for s in sessions(DATABASE).await {
        let set = &s
            .execute(
                "SELECT ID, post_title FROM wp_posts WHERE 1 = 0",
                10,
                CELLS,
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
    for s in sessions(DATABASE).await {
        let r = s
            .execute(
                "CREATE TEMPORARY TABLE tmp (id int AUTO_INCREMENT PRIMARY KEY, v int); \
                 INSERT INTO tmp (v) VALUES (1), (2), (3); \
                 UPDATE tmp SET v = 0 WHERE v > 1; \
                 SELECT COUNT(*) FROM tmp",
                10,
                CELLS,
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
    for s in sessions(DATABASE).await {
        let err = s
            .execute("SELECT * FROM missing_table", 10, CELLS, TIMEOUT)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Database);
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "1146");
    }
}

#[tokio::test]
#[ignore]
async fn use_changes_the_current_database_and_session_state_persists() {
    for s in sessions(DATABASE).await {
        let r = s.execute("USE mysql", 10, CELLS, TIMEOUT).await.unwrap();
        assert_eq!(r.database.as_deref(), Some("mysql"));
        s.execute("SET @x = 42", 10, CELLS, TIMEOUT).await.unwrap();
        let r = s.execute("SELECT @x", 10, CELLS, TIMEOUT).await.unwrap();
        assert_eq!(text(&r.results[0].rows[0][0]), "42");
        s.use_database("databee_test").await.unwrap();
        assert_eq!(s.current_database().as_deref(), Some("databee_test"));
    }
}

#[tokio::test]
#[ignore]
async fn timeout_aborts_and_session_recovers() {
    for (host, port) in servers() {
        let s = connect_to(&host, port, DATABASE).await;
        s.execute("USE mysql", 10, CELLS, TIMEOUT).await.unwrap();

        let err = s
            .execute("SELECT SLEEP(5)", 10, CELLS, Duration::from_secs(1))
            .await
            .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::Timeout, "{host}:{port}: {err:?}");
        assert!(err.session_reset(), "{host}:{port}: {err:?}");
        let r = s
            .execute("SELECT DATABASE()", 10, CELLS, TIMEOUT)
            .await
            .unwrap();
        assert_eq!(text(&r.results[0].rows[0][0]), "mysql", "{host}:{port}");
        assert_eq!(s.current_database().as_deref(), Some("mysql"));
    }
}

#[tokio::test]
#[ignore]
async fn cancel_interrupts_a_running_query() {
    for (host, port) in servers() {
        let s = Arc::new(connect_to(&host, port, DATABASE).await);
        let observer = connect_to(&host, port, DATABASE).await;
        let runner = {
            let s = s.clone();
            tokio::spawn(async move {
                s.execute(
                    "SELECT SLEEP(30) /* cancel_probe */",
                    10,
                    CELLS,
                    Duration::from_secs(60),
                )
                .await
            })
        };
        assert!(
            wait_for_marker_running(&observer, "cancel_", "probe", 1).await,
            "{host}:{port}: the query never started"
        );

        let started = std::time::Instant::now();
        s.cancel().await.unwrap();
        // MySQL reports an interrupted SLEEP as a result row (1); MariaDB as error 1317.
        match runner.await.unwrap() {
            Err(e) => assert_eq!(e.kind(), ErrorKind::Cancelled, "{host}:{port}"),
            Ok(r) => assert_eq!(text(&r.results[0].rows[0][0]), "1", "{host}:{port}"),
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{host}:{port}: cancel took {:?}",
            started.elapsed()
        );
        let r = s
            .execute("SELECT 1", 10, CELLS, TIMEOUT)
            .await
            .unwrap_or_else(|e| panic!("{host}:{port}: {e:?}"));
        assert_eq!(text(&r.results[0].rows[0][0]), "1", "{host}:{port}");
    }
}

#[tokio::test]
#[ignore]
async fn with_delete_keeps_its_affected_rows() {
    for s in sessions(DATABASE).await {
        s.execute("DROP TABLE IF EXISTS cte_delete", 10, CELLS, TIMEOUT)
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
                CELLS,
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
    for s in sessions(DATABASE).await {
        s.execute(
            "DROP TABLE IF EXISTS ddl_probe; CREATE TABLE ddl_probe (id int PRIMARY KEY)",
            10,
            CELLS,
            TIMEOUT,
        )
        .await
        .unwrap();
        let st = s.structure("databee_test", "ddl_probe").await;
        s.execute("DROP TABLE ddl_probe", 10, CELLS, TIMEOUT)
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
        let s = Arc::new(connect_to(&host, port, DATABASE).await);
        let observer = connect_to(&host, port, DATABASE).await;
        let busy: Vec<_> = (0..3)
            .map(|_| {
                let s = s.clone();
                tokio::spawn(
                    async move { s.fetch("SELECT SLEEP(5) /* pool_probe */", TIMEOUT).await },
                )
            })
            .collect();
        let runner = {
            let s = s.clone();
            tokio::spawn(async move {
                s.execute("SELECT SLEEP(10) /* editor_probe */", 10, CELLS, TIMEOUT)
                    .await
            })
        };
        assert!(
            wait_for_marker_running(&observer, "pool_", "probe", 3).await,
            "{host}:{port}: the pool connections never got busy"
        );
        assert!(
            wait_for_marker_running(&observer, "editor_", "probe", 1).await,
            "{host}:{port}: the editor query never started"
        );

        let started = std::time::Instant::now();
        s.cancel()
            .await
            .unwrap_or_else(|e| panic!("{host}:{port}: {e:?}"));
        match runner.await.unwrap() {
            Err(e) => assert_eq!(e.kind(), ErrorKind::Cancelled, "{host}:{port}"),
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
async fn close_ends_a_running_query_and_later_executes_fail_with_a_connection_error() {
    for (host, port) in servers() {
        let s = Arc::new(connect_to(&host, port, DATABASE).await);
        let observer = connect_to(&host, port, DATABASE).await;
        let runner = {
            let s = s.clone();
            tokio::spawn(async move {
                s.execute(
                    "SELECT SLEEP(30) /* close_probe */",
                    10,
                    CELLS,
                    Duration::from_secs(60),
                )
                .await
            })
        };
        assert!(
            wait_for_marker_running(&observer, "close_", "probe", 1).await,
            "{host}:{port}: the query never started"
        );

        let started = std::time::Instant::now();
        s.close().await;
        runner.await.unwrap().ok();
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{host}:{port}: close took {:?}",
            started.elapsed()
        );
        assert!(
            wait_for_marker_gone(&observer, "close_", "probe").await,
            "{host}:{port}: the query still runs after close"
        );

        let err = s.execute("SELECT 1", 10, CELLS, TIMEOUT).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Connection, "{host}:{port}: {err:?}");
    }
}

#[tokio::test]
#[ignore]
async fn killed_editor_connection_fails_once_then_reconnects() {
    for (host, port) in servers() {
        let s = connect_to(&host, port, DATABASE).await;
        let other = connect_to(&host, port, DATABASE).await;
        let id = s
            .execute("SELECT CONNECTION_ID()", 10, CELLS, TIMEOUT)
            .await
            .unwrap();
        let id = text(&id.results[0].rows[0][0]).to_string();
        other
            .execute(&format!("KILL {id}"), 10, CELLS, TIMEOUT)
            .await
            .unwrap();
        assert!(
            wait_for_processes(&other, &format!("ID = {id}"), |n| n == 0).await,
            "{host}:{port}: the killed connection is still listed"
        );

        let err = s.execute("SELECT 1", 10, CELLS, TIMEOUT).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Connection, "{host}:{port}: {err:?}");
        let r = s
            .execute("SELECT 1", 10, CELLS, TIMEOUT)
            .await
            .unwrap_or_else(|e| panic!("{host}:{port}: {e:?}"));
        assert_eq!(text(&r.results[0].rows[0][0]), "1", "{host}:{port}");
    }
}

#[tokio::test]
#[ignore]
async fn browse_pages_with_order_and_filter() {
    for s in sessions(DATABASE).await {
        let req = BrowseRequest {
            database: "databee_test".into(),
            table: "wp_posts".into(),
            limit: 2,
            order_by: Some("ID".into()),
            descending: true,
            ..Default::default()
        };
        let first = s.browse(&req, CELLS, TIMEOUT).await.unwrap();
        assert!(first.has_more);
        assert_eq!(text(&first.result.rows[0][0]), "3");

        let second = s
            .browse(
                &BrowseRequest {
                    offset: 2,
                    ..req.clone()
                },
                CELLS,
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
                CELLS,
                TIMEOUT,
            )
            .await
            .unwrap();
        assert_eq!(filtered.result.rows.len(), 1);
    }
}

#[tokio::test]
#[ignore]
async fn long_text_arrives_as_a_preview_and_the_cell_cache_returns_the_full_value() {
    let full = "ä".repeat(5000);
    for (host, port) in servers() {
        let s = connect_to(&host, port, DATABASE).await;
        let mut response = s
            .execute(
                "SELECT 'short', REPEAT('ä', 5000)",
                10,
                CellLimit(100),
                TIMEOUT,
            )
            .await
            .unwrap_or_else(|e| panic!("{host}:{port}: {e:?}"));

        let set = &mut response.results[0];
        assert_eq!(
            set.rows[0],
            [
                Cell::Text("short".into()),
                Cell::LongText {
                    len: 5000,
                    preview: "ä".repeat(100)
                }
            ],
            "{host}:{port}"
        );
        let cache = CellCache::default();
        cache.keep(set);
        assert_eq!(cache.get(set.id, 0, 1).unwrap(), full, "{host}:{port}");
        assert_eq!(
            cache.get(set.id, 0, 0).unwrap_err().kind(),
            ErrorKind::InvalidInput,
            "{host}:{port}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn failing_statement_keeps_earlier_results_and_reports_its_index() {
    for s in sessions(DATABASE).await {
        let response = s
            .execute(
                "SELECT 1; SELECT * FROM missing_table; SELECT 3",
                10,
                CELLS,
                TIMEOUT,
            )
            .await
            .unwrap();

        assert_eq!(response.results.len(), 1);
        assert_eq!(text(&response.results[0].rows[0][0]), "1");
        let failure = response.failure.expect("a failure");
        assert_eq!(failure.index, 1);
        assert_eq!(failure.error.kind(), ErrorKind::Database);
        assert_eq!(failure.error.code(), Some("1146"));
    }
}

#[tokio::test]
#[ignore]
async fn timeout_reconnect_reports_the_session_reset_and_drops_session_state() {
    for (host, port) in servers() {
        let s = connect_to(&host, port, DATABASE).await;
        s.execute("SET @x = 42", 10, CELLS, TIMEOUT).await.unwrap();

        let err = s
            .execute("SELECT SLEEP(5)", 10, CELLS, Duration::from_secs(1))
            .await
            .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::Timeout, "{host}:{port}: {err:?}");
        assert!(err.session_reset(), "{host}:{port}: {err:?}");
        let r = s.execute("SELECT @x", 10, CELLS, TIMEOUT).await.unwrap();
        assert_eq!(r.results[0].rows[0][0], Cell::Null, "{host}:{port}");
    }
}

/// Polls the server's process list every 50 ms for up to 5 s until the number of threads
/// matching `condition` satisfies `done`.
async fn wait_for_processes(
    observer: &MySqlSession,
    condition: &str,
    done: impl Fn(u64) -> bool,
) -> bool {
    let sql = format!("SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE {condition}");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let r = observer.execute(&sql, 10, CELLS, TIMEOUT).await.unwrap();
        let count: u64 = text(&r.results[0].rows[0][0]).parse().unwrap();
        if done(count) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Matches threads running a statement containing `prefix` + `suffix`. The marker is split so
/// the observer's own text does not match.
fn marker_condition(prefix: &str, suffix: &str) -> String {
    format!("INFO LIKE CONCAT('%', '{prefix}', '{suffix}', '%')")
}

async fn wait_for_marker_gone(observer: &MySqlSession, prefix: &str, suffix: &str) -> bool {
    wait_for_processes(observer, &marker_condition(prefix, suffix), |n| n == 0).await
}

async fn wait_for_marker_running(
    observer: &MySqlSession,
    prefix: &str,
    suffix: &str,
    at_least: u64,
) -> bool {
    wait_for_processes(observer, &marker_condition(prefix, suffix), |n| {
        n >= at_least
    })
    .await
}

#[tokio::test]
#[ignore]
async fn browse_that_times_out_is_killed_on_the_server() {
    for (host, port) in servers() {
        let s = connect_to(&host, port, DATABASE).await;
        let observer = connect_to(&host, port, DATABASE).await;
        let req = BrowseRequest {
            database: "databee_test".into(),
            table: "wp_posts".into(),
            limit: 10,
            filter: Some("SLEEP(30) = 0 AND 'kill_probe_browse' <> ''".into()),
            ..Default::default()
        };

        let err = s
            .browse(&req, CELLS, Duration::from_secs(1))
            .await
            .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::Timeout, "{host}:{port}: {err:?}");
        assert!(
            wait_for_marker_gone(&observer, "kill_probe_", "browse").await,
            "{host}:{port}: the browse query still runs"
        );
    }
}

#[tokio::test]
#[ignore]
async fn fetch_that_times_out_is_killed_on_the_server() {
    for (host, port) in servers() {
        let s = connect_to(&host, port, DATABASE).await;
        let observer = connect_to(&host, port, DATABASE).await;

        let err = s
            .fetch(
                "SELECT SLEEP(30) /* kill_probe_fetch */",
                Duration::from_secs(1),
            )
            .await
            .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::Timeout, "{host}:{port}: {err:?}");
        assert!(
            wait_for_marker_gone(&observer, "kill_probe_", "fetch").await,
            "{host}:{port}: the fetch query still runs"
        );
    }
}

#[tokio::test]
#[ignore]
async fn call_returns_each_result_set_of_the_procedure() {
    for (host, port) in servers() {
        let s = connect_to(&host, port, DATABASE).await;
        s.execute(
            "DROP PROCEDURE IF EXISTS two_result_sets",
            10,
            CELLS,
            TIMEOUT,
        )
        .await
        .unwrap();
        s.execute(
            "CREATE PROCEDURE two_result_sets() BEGIN SELECT 1 AS a; SELECT 'x' AS b, 2 AS c; END",
            10,
            CELLS,
            TIMEOUT,
        )
        .await
        .unwrap_or_else(|e| panic!("{host}:{port}: {e:?}"));

        let r = s
            .execute("CALL two_result_sets()", 10, CELLS, TIMEOUT)
            .await
            .unwrap_or_else(|e| panic!("{host}:{port}: {e:?}"));

        let names = |i: usize| -> Vec<&str> {
            r.results[i]
                .columns
                .iter()
                .map(|c| c.name.as_str())
                .collect()
        };
        assert!(r.results.len() >= 2, "{host}:{port}: {:?}", r.results);
        assert_eq!(names(0), ["a"], "{host}:{port}");
        assert_eq!(r.results[0].rows.len(), 1, "{host}:{port}");
        assert_eq!(text(&r.results[0].rows[0][0]), "1", "{host}:{port}");
        assert_eq!(names(1), ["b", "c"], "{host}:{port}");
        assert_eq!(r.results[1].rows.len(), 1, "{host}:{port}");
        assert_eq!(text(&r.results[1].rows[0][0]), "x", "{host}:{port}");
        assert_eq!(text(&r.results[1].rows[0][1]), "2", "{host}:{port}");
        assert!(
            r.results[2..].iter().all(|set| set.rows.is_empty()),
            "{host}:{port}: {:?}",
            r.results
        );
    }
}

#[tokio::test]
#[ignore]
async fn select_into_a_variable_sets_it_for_the_next_statement() {
    for s in sessions(DATABASE).await {
        let r = s
            .execute(
                "SELECT 42 INTO @into_probe; SELECT @into_probe",
                10,
                CELLS,
                TIMEOUT,
            )
            .await
            .unwrap();

        assert_eq!(r.results.len(), 2, "{}", s.server_version());
        assert!(r.results[0].rows.is_empty(), "{}", s.server_version());
        assert_eq!(
            text(&r.results[1].rows[0][0]),
            "42",
            "{}",
            s.server_version()
        );
    }
}

#[tokio::test]
#[ignore]
async fn unknown_database_fails_and_leaves_the_session_usable() {
    for (host, port) in servers() {
        let s = connect_to(&host, port, DATABASE).await;

        let err = s.use_database("missing").await.unwrap_err();

        assert_eq!(err.kind(), ErrorKind::Database, "{host}:{port}: {err:?}");
        assert_eq!(err.code(), Some("1049"), "{host}:{port}: {err:?}");
        assert_eq!(s.current_database().as_deref(), Some(DATABASE));
        let r = s
            .execute("SELECT DATABASE()", 10, CELLS, TIMEOUT)
            .await
            .unwrap();
        assert_eq!(text(&r.results[0].rows[0][0]), DATABASE, "{host}:{port}");
    }
}

#[tokio::test]
#[ignore]
async fn browse_filter_accepts_id_shorthand_and_trailing_hash_comments() {
    let cases = [
        ("2", FilterKind::Ids, "`ID` = 2", vec!["2"]),
        (
            "1-2",
            FilterKind::Ids,
            "`ID` BETWEEN 1 AND 2",
            vec!["1", "2"],
        ),
        ("1, 3", FilterKind::Ids, "`ID` IN (1, 3)", vec!["1", "3"]),
        (
            "post_type = 'page' # only pages",
            FilterKind::Sql,
            "post_type = 'page' # only pages",
            vec!["2"],
        ),
    ];
    for s in sessions(DATABASE).await {
        for (filter, kind, applied_sql, ids) in &cases {
            let page = s
                .browse(
                    &BrowseRequest {
                        database: "databee_test".into(),
                        table: "wp_posts".into(),
                        limit: 10,
                        order_by: Some("ID".into()),
                        filter: Some(filter.to_string()),
                        primary_key: Some("ID".into()),
                        ..Default::default()
                    },
                    CELLS,
                    TIMEOUT,
                )
                .await
                .unwrap_or_else(|e| panic!("{filter}: {e:?}"));

            let applied = page.filter.as_ref().expect("an applied filter");
            assert_eq!(applied.kind, *kind, "{filter}");
            assert_eq!(applied.sql, *applied_sql, "{filter}");
            let got: Vec<&str> = page.result.rows.iter().map(|row| text(&row[0])).collect();
            assert_eq!(got, ids.as_slice(), "{filter}");
            assert!(!page.has_more, "{filter}");
        }
    }
}

#[tokio::test]
#[ignore]
async fn structure_of_a_view_lists_its_columns_without_keys() {
    for s in sessions(DATABASE).await {
        let st = s.structure("databee_test", "published").await.unwrap();

        let names: Vec<_> = st.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["ID", "post_title"], "{}", s.server_version());
        assert!(st.indexes.is_empty(), "{}", s.server_version());
        assert!(st.primary_key.is_empty(), "{}", s.server_version());
        let ddl = st.ddl.as_deref().unwrap_or_default();
        assert!(
            ddl.starts_with("CREATE") && ddl.contains("VIEW `published`"),
            "{}: {ddl}",
            s.server_version()
        );
    }
}

#[tokio::test]
#[ignore]
async fn fetch_stops_at_fifty_thousand_rows_and_flags_the_cut() {
    // 250 x 250 rows from a CTE that stays within the default recursion limits.
    let sql = "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 250) \
               SELECT a.i FROM n a CROSS JOIN n b";
    for s in sessions(DATABASE).await {
        let set = s
            .fetch(sql, Duration::from_secs(60))
            .await
            .unwrap_or_else(|e| panic!("{}: {e:?}", s.server_version()));

        assert_eq!(set.rows.len(), 50_000, "{}", s.server_version());
        assert_eq!(set.total_rows, 62_500, "{}", s.server_version());
        assert!(set.truncated, "{}", s.server_version());
    }
}

#[tokio::test]
#[ignore]
async fn apply_rolls_back_the_first_statement_when_the_second_fails() {
    for (host, port) in servers() {
        let s = connect_to(&host, port, DATABASE).await;
        s.execute(
            "DROP TABLE IF EXISTS apply_rollback_probe; \
             CREATE TABLE apply_rollback_probe (id int PRIMARY KEY) ENGINE=InnoDB",
            10,
            CELLS,
            TIMEOUT,
        )
        .await
        .unwrap();

        let err = s
            .apply(
                &[
                    "INSERT INTO apply_rollback_probe VALUES (1)".to_string(),
                    "INSERT INTO apply_rollback_probe VALUES (1)".to_string(),
                ],
                TIMEOUT,
            )
            .await
            .unwrap_err();
        let remaining = s
            .fetch("SELECT COUNT(*) FROM apply_rollback_probe", TIMEOUT)
            .await;
        s.execute("DROP TABLE apply_rollback_probe", 10, CELLS, TIMEOUT)
            .await
            .unwrap();

        assert_eq!(err.kind(), ErrorKind::Database, "{host}:{port}: {err:?}");
        assert_eq!(err.code(), Some("1062"), "{host}:{port}: {err:?}");
        assert_eq!(text(&remaining.unwrap().rows[0][0]), "0", "{host}:{port}");
    }
}

#[tokio::test]
#[ignore]
async fn completion_maps_tables_to_columns() {
    for s in sessions(DATABASE).await {
        let map = s.completion("databee_test").await.unwrap();
        assert_eq!(map["published"], ["ID", "post_title"]);
    }
}
