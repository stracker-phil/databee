//! Connection health against the Docker fixtures in `dev/docker-compose.yml` (`bun run test:db`):
//! a killed editor connection is replaced with its session state restored, and a server that
//! stops answering fails `ping` and `reconnect` with a connection error. The containers are never
//! stopped; the second case reaches the server through a local TCP relay that the test closes.

use std::time::{Duration, Instant};

use databee_lib::connection::Secret;
use databee_lib::db::mysql::MySqlSession;
use databee_lib::db::{Cell, CellLimit, Database};
use databee_lib::error::{AppError, ErrorKind};
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::{JoinHandle, JoinSet};

mod common;

use common::{connect_to, servers, spec};

const DATABASE: &str = "databee_test";
const WRITABLE: &str = "wp_write";
const TIMEOUT: Duration = Duration::from_secs(10);
const CELLS: CellLimit = CellLimit::NONE;

fn text(c: &Cell) -> &str {
    match c {
        Cell::Number(s) | Cell::Text(s) | Cell::Temporal(s) | Cell::Json(s) => s,
        other => panic!("not a text-like cell: {other:?}"),
    }
}

async fn row(db: &MySqlSession, sql: &str) -> Vec<String> {
    let response = db.execute(sql, 10, CELLS, TIMEOUT).await.unwrap();
    response.results[0].rows[0]
        .iter()
        .map(|cell| text(cell).to_string())
        .collect()
}

async fn wait_until_gone(observer: &MySqlSession, id: &str) -> bool {
    let sql = format!("SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE ID = {id}");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if row(observer, &sql).await[0] == "0" {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// A session whose editor connection changed variables and database, then was killed.
async fn killed_session(host: &str, port: u16) -> MySqlSession {
    let session = connect_to(host, port, DATABASE).await;
    let observer = connect_to(host, port, DATABASE).await;
    for sql in [
        "SET SESSION sql_mode='ANSI_QUOTES', time_zone='+03:00', foreign_key_checks=0",
        "USE wp_write",
    ] {
        session
            .execute(sql, 10, CELLS, TIMEOUT)
            .await
            .unwrap_or_else(|e| panic!("{host}:{port}: {sql}: {e:?}"));
    }
    let id = row(&session, "SELECT CONNECTION_ID()").await.remove(0);
    observer
        .execute(&format!("KILL {id}"), 10, CELLS, TIMEOUT)
        .await
        .unwrap();
    assert!(
        wait_until_gone(&observer, &id).await,
        "{host}:{port}: the killed connection is still listed"
    );
    session
}

async fn assert_state_restored(session: &MySqlSession, label: &str) {
    let state = row(
        session,
        "SELECT @@SESSION.sql_mode, @@SESSION.time_zone, @@SESSION.foreign_key_checks, DATABASE()",
    )
    .await;
    assert_eq!(state, ["ANSI_QUOTES", "+03:00", "0", WRITABLE], "{label}");
    assert_eq!(
        session.current_database().as_deref(),
        Some(WRITABLE),
        "{label}"
    );
    assert!(
        session.dialect().backslash_escapes,
        "{label}: the dialect follows the server default, not the session's sql_mode"
    );
}

#[tokio::test]
#[ignore]
async fn ping_replaces_a_killed_editor_connection_and_restores_its_session_state() {
    for (host, port) in servers() {
        let session = killed_session(&host, port).await;

        session
            .ping()
            .await
            .unwrap_or_else(|e| panic!("{host}:{port}: {e:?}"));

        assert_state_restored(&session, &format!("{host}:{port}")).await;
    }
}

#[tokio::test]
#[ignore]
async fn reconnect_replaces_a_killed_editor_connection_and_restores_its_session_state() {
    for (host, port) in servers() {
        let session = killed_session(&host, port).await;

        session
            .reconnect()
            .await
            .unwrap_or_else(|e| panic!("{host}:{port}: {e:?}"));

        assert_state_restored(&session, &format!("{host}:{port}")).await;
    }
}

#[tokio::test]
#[ignore]
async fn ping_and_reconnect_leave_a_healthy_session_as_it_was() {
    for (host, port) in servers() {
        let session = connect_to(&host, port, DATABASE).await;
        session
            .execute("SET SESSION foreign_key_checks=0", 10, CELLS, TIMEOUT)
            .await
            .unwrap();
        let before = row(&session, "SELECT CONNECTION_ID()").await;

        session.ping().await.unwrap();
        session.reconnect().await.unwrap();

        let label = format!("{host}:{port}");
        assert_eq!(
            row(&session, "SELECT CONNECTION_ID()").await,
            before,
            "{label}"
        );
        assert_eq!(
            row(&session, "SELECT @@SESSION.foreign_key_checks").await,
            ["0"],
            "{label}"
        );
    }
}

/// Forwards connections on an OS-assigned loopback port to `target` until `close`.
struct Relay {
    port: u16,
    task: JoinHandle<()>,
}

impl Relay {
    async fn open(target: (String, u16)) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            while let Ok((mut client, _)) = listener.accept().await {
                let target = target.clone();
                connections.spawn(async move {
                    if let Ok(mut server) = TcpStream::connect(target).await {
                        let _ = copy_bidirectional(&mut client, &mut server).await;
                    }
                });
            }
        });
        Self { port, task }
    }

    async fn close(self) {
        self.task.abort();
        let _ = self.task.await;
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[tokio::test]
#[ignore]
async fn ping_and_reconnect_fail_with_a_connection_error_once_the_server_is_gone() {
    for (host, port) in servers() {
        let relay = Relay::open((host.clone(), port)).await;
        let session = MySqlSession::connect(
            &spec("127.0.0.1", relay.port, DATABASE),
            Some(&Secret::new("root")),
            TIMEOUT,
            TIMEOUT,
        )
        .await
        .unwrap_or_else(|e| panic!("{host}:{port} through the relay: {e:?}"));
        session.ping().await.unwrap();
        relay.close().await;

        for (call, (elapsed, result)) in [
            ("ping", timed(session.ping()).await),
            ("reconnect", timed(session.reconnect()).await),
        ] {
            let err = result.expect_err(&format!("{host}:{port}: {call} succeeded"));
            assert_eq!(
                err.kind(),
                ErrorKind::Connection,
                "{host}:{port}: {call}: {err:?}"
            );
            assert!(
                elapsed < Duration::from_secs(6),
                "{host}:{port}: {call} took {elapsed:?}"
            );
        }
    }
}

async fn timed<T>(
    call: impl std::future::Future<Output = Result<T, AppError>>,
) -> (Duration, Result<T, AppError>) {
    let started = Instant::now();
    let result = call.await;
    (started.elapsed(), result)
}
