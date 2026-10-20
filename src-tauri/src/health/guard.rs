use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use super::{SessionHealth, check};
use crate::db::{
    BrowsePage, BrowseRequest, CellLimit, Database, DatabaseInfo, Dialect, ExecuteResponse,
    ResultSet, TableInfo, TableKeys, TableStructure,
};
use crate::error::{AppError, AppResult};

/// A session's database as every caller sees it: while the session is not `ok`, a call that
/// needs the server fails at once with `ConnectionLost` instead of waiting on a dead socket, and
/// an error that suggests a lost connection starts a check right away.
pub struct Guarded {
    inner: Arc<dyn Database>,
    health: Arc<SessionHealth>,
}

impl Guarded {
    pub fn new(inner: Box<dyn Database>, health: Arc<SessionHealth>) -> Self {
        Self {
            inner: Arc::from(inner),
            health,
        }
    }

    fn admit(&self) -> AppResult<()> {
        self.health.refusal().map_or(Ok(()), Err)
    }

    fn observe<T>(&self, result: AppResult<T>) -> AppResult<T> {
        if let Err(error) = &result {
            self.suspect(error);
        }
        result
    }

    /// The check runs apart from the call, so the caller gets its error without waiting for a ping.
    fn suspect(&self, error: &AppError) {
        if !error.suggests_connection_loss() {
            return;
        }
        let (db, health) = (self.inner.clone(), self.health.clone());
        tokio::spawn(async move { check(db.as_ref(), &health).await });
    }
}

#[async_trait]
impl Database for Guarded {
    fn dialect(&self) -> Dialect {
        self.inner.dialect()
    }

    fn server_version(&self) -> &str {
        self.inner.server_version()
    }

    fn current_database(&self) -> Option<String> {
        self.inner.current_database()
    }

    async fn close(&self) {
        self.inner.close().await
    }

    async fn execute(
        &self,
        sql: &str,
        row_limit: usize,
        cell_limit: CellLimit,
        timeout: Duration,
    ) -> AppResult<ExecuteResponse> {
        self.admit()?;
        let response = self.observe(
            self.inner
                .execute(sql, row_limit, cell_limit, timeout)
                .await,
        )?;
        if let Some(failure) = &response.failure {
            self.suspect(&failure.error);
        }
        Ok(response)
    }

    async fn cancel(&self) -> AppResult<()> {
        self.inner.cancel().await
    }

    async fn use_database(&self, database: &str) -> AppResult<()> {
        self.admit()?;
        self.observe(self.inner.use_database(database).await)
    }

    async fn databases(&self) -> AppResult<Vec<DatabaseInfo>> {
        self.admit()?;
        self.observe(self.inner.databases().await)
    }

    async fn tables(&self, database: &str) -> AppResult<Vec<TableInfo>> {
        self.admit()?;
        self.observe(self.inner.tables(database).await)
    }

    async fn structure(&self, database: &str, table: &str) -> AppResult<TableStructure> {
        self.admit()?;
        self.observe(self.inner.structure(database, table).await)
    }

    async fn keys(&self, database: &str) -> AppResult<BTreeMap<String, TableKeys>> {
        self.admit()?;
        self.observe(self.inner.keys(database).await)
    }

    async fn completion(&self, database: &str) -> AppResult<BTreeMap<String, Vec<String>>> {
        self.admit()?;
        self.observe(self.inner.completion(database).await)
    }

    async fn browse(
        &self,
        req: &BrowseRequest,
        cell_limit: CellLimit,
        timeout: Duration,
    ) -> AppResult<BrowsePage> {
        self.admit()?;
        self.observe(self.inner.browse(req, cell_limit, timeout).await)
    }

    async fn fetch(&self, sql: &str, timeout: Duration) -> AppResult<ResultSet> {
        self.admit()?;
        self.observe(self.inner.fetch(sql, timeout).await)
    }

    async fn apply(&self, statements: &[String], timeout: Duration) -> AppResult<u64> {
        self.admit()?;
        self.observe(self.inner.apply(statements, timeout).await)
    }

    async fn ping(&self) -> AppResult<()> {
        self.inner.ping().await
    }

    async fn reconnect(&self) -> AppResult<()> {
        self.inner.reconnect().await
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::broadcast::Receiver;
    use tokio::sync::broadcast::error::TryRecvError;

    use super::*;
    use crate::db::{FakeDatabase, ResultSet};
    use crate::error::ErrorKind;
    use crate::health::tests::{drain, go_down, refused, registered, spec};
    use crate::health::{Health, HealthEvent, retry};
    use crate::session::Session;

    const TIMEOUT: Duration = Duration::from_secs(1);
    const SETTLE: Duration = Duration::from_millis(100);

    fn canned() -> Arc<FakeDatabase> {
        Arc::new(FakeDatabase::new().on("", ResultSet::default()))
    }

    async fn run(session: &Session) -> AppResult<ExecuteResponse> {
        session
            .db
            .execute("SELECT 1", 10, CellLimit::NONE, TIMEOUT)
            .await
    }

    async fn next_health(events: &mut Receiver<HealthEvent>) -> Health {
        tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("no health event arrived")
            .unwrap()
            .health
    }

    async fn stays_ok(session: &Session, events: &mut Receiver<HealthEvent>) -> bool {
        tokio::time::sleep(SETTLE).await;
        session.health.get() == Health::Ok && matches!(events.try_recv(), Err(TryRecvError::Empty))
    }

    #[tokio::test]
    async fn a_down_session_refuses_every_call_that_needs_the_server() {
        let db = canned();
        let (session, _events) = registered(&db);
        go_down(&db, &session).await;
        let sql = ["DELETE FROM t".to_string()];
        let request = BrowseRequest {
            database: "wp".into(),
            table: "t".into(),
            limit: 10,
            ..BrowseRequest::default()
        };
        let calls = &session.db;

        let refusals = [
            ("execute", run(&session).await.err()),
            ("use_database", calls.use_database("wp").await.err()),
            ("databases", calls.databases().await.err()),
            ("tables", calls.tables("wp").await.err()),
            ("structure", calls.structure("wp", "t").await.err()),
            ("keys", calls.keys("wp").await.err()),
            ("completion", calls.completion("wp").await.err()),
            (
                "browse",
                calls.browse(&request, CellLimit::NONE, TIMEOUT).await.err(),
            ),
            ("fetch", calls.fetch("SELECT 1", TIMEOUT).await.err()),
            ("apply", calls.apply(&sql, TIMEOUT).await.err()),
        ];

        for (name, err) in refusals {
            assert_eq!(
                err.map(|e| e.kind()),
                Some(ErrorKind::ConnectionLost),
                "{name}"
            );
        }
        assert!(db.queries().is_empty());
        assert!(db.applied().is_empty());
        assert_eq!(db.current_database(), None);
    }

    #[tokio::test]
    async fn the_refusal_carries_the_error_that_took_the_session_down() {
        let db = canned();
        let (session, _events) = registered(&db);
        go_down(&db, &session).await;

        let err = run(&session).await.unwrap_err();

        assert_eq!(
            err,
            AppError::ConnectionLost("Could not connect: refused".into())
        );
        assert_eq!(
            err.to_string(),
            "Connection lost: Could not connect: refused"
        );
    }

    #[tokio::test]
    async fn a_reconnecting_session_refuses_calls_too() {
        let db = canned();
        let (session, _events) = registered(&db);
        go_down(&db, &session).await;

        let attempt = session.health.begin_retry().unwrap();
        let err = run(&session).await.unwrap_err();
        drop(attempt);

        assert_eq!(err.kind(), ErrorKind::ConnectionLost);
        assert!(db.queries().is_empty());
    }

    #[tokio::test]
    async fn ping_reconnect_cancel_and_the_plain_getters_pass_through_while_down() {
        let db = Arc::new(FakeDatabase::new().with_database("wp"));
        let (session, _events) = registered(&db);
        go_down(&db, &session).await;
        db.set_unreachable(None);

        assert!(session.db.ping().await.is_ok());
        assert!(session.db.reconnect().await.is_ok());
        assert!(session.db.cancel().await.is_ok());
        assert_eq!(session.db.server_version(), "fake");
        assert_eq!(session.db.current_database().as_deref(), Some("wp"));
    }

    #[tokio::test]
    async fn calls_work_again_once_a_retry_has_reconnected() {
        let db = canned();
        let (session, _events) = registered(&db);
        go_down(&db, &session).await;
        db.set_unreachable(None);

        retry(session.db.as_ref(), &session.health).await;

        assert!(run(&session).await.is_ok());
        assert_eq!(db.queries(), ["SELECT 1"]);
    }

    #[tokio::test]
    async fn a_session_that_was_never_registered_is_not_refused_while_down() {
        let db = canned();
        let session = Session::new("a".into(), spec(), "local".into(), Box::new(db.clone()));
        let down = go_down(&db, &session).await;

        let result = run(&session).await;

        assert!(matches!(down, Health::Down { .. }));
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(db.queries(), ["SELECT 1"]);
    }

    #[tokio::test]
    async fn a_connection_or_timeout_error_marks_an_unreachable_session_down() {
        for error in [
            AppError::Connection("reset".into()),
            AppError::timeout(Duration::from_secs(1)),
        ] {
            let db = Arc::new(FakeDatabase::new().failing("SELECT 1", error.clone()));
            let (session, mut events) = registered(&db);
            db.set_unreachable(Some(refused()));

            let got = run(&session).await.unwrap_err();
            let health = next_health(&mut events).await;

            assert_eq!(got, error);
            assert!(
                matches!(health, Health::Down { .. }),
                "{error:?}: {health:?}"
            );
            assert_eq!(session.health.get(), health, "{error:?}");
        }
    }

    #[tokio::test]
    async fn a_failing_fetch_marks_an_unreachable_session_down() {
        let reset = AppError::Connection("reset".into());
        let db = Arc::new(FakeDatabase::new().failing("SELECT 1", reset));
        let (session, mut events) = registered(&db);
        db.set_unreachable(Some(refused()));

        session.db.fetch("SELECT 1", TIMEOUT).await.unwrap_err();

        assert!(matches!(
            next_health(&mut events).await,
            Health::Down { .. }
        ));
    }

    #[tokio::test]
    async fn a_connection_error_leaves_a_reachable_session_ok() {
        for error in [
            AppError::Connection("reset".into()),
            AppError::timeout(Duration::from_secs(1)),
        ] {
            let db = Arc::new(FakeDatabase::new().failing("SELECT 1", error.clone()));
            let (session, mut events) = registered(&db);

            run(&session).await.unwrap_err();

            assert!(stays_ok(&session, &mut events).await, "{error:?}");
        }
    }

    #[tokio::test]
    async fn errors_that_do_not_suggest_a_lost_connection_start_no_check() {
        for error in [
            AppError::InvalidInput("bad".into()),
            AppError::Database {
                message: "no table".into(),
                code: Some("1146".into()),
            },
            AppError::Cancelled,
        ] {
            let db = Arc::new(FakeDatabase::new().failing("SELECT 1", error.clone()));
            let (session, mut events) = registered(&db);
            db.set_unreachable(Some(refused()));

            let got = run(&session).await.unwrap_err();

            assert_eq!(got, error);
            assert!(stays_ok(&session, &mut events).await, "{error:?}");
        }
    }

    #[tokio::test]
    async fn a_script_that_fails_midway_with_a_lost_connection_marks_the_session_down() {
        let db = Arc::new(
            FakeDatabase::new().failing_midway("SELECT 2", AppError::Connection("reset".into())),
        );
        let (session, mut events) = registered(&db);
        db.set_unreachable(Some(refused()));

        let response = session
            .db
            .execute("SELECT 1; SELECT 2", 10, CellLimit::NONE, TIMEOUT)
            .await
            .unwrap();
        let health = next_health(&mut events).await;

        assert!(response.failure.is_some());
        assert!(matches!(health, Health::Down { .. }), "{health:?}");
        assert!(drain(&mut events).is_empty());
    }

    #[tokio::test]
    async fn a_script_that_fails_midway_with_a_sql_error_starts_no_check() {
        let db = Arc::new(FakeDatabase::new().failing_midway(
            "SELECT 2",
            AppError::Database {
                message: "no table".into(),
                code: Some("1146".into()),
            },
        ));
        let (session, mut events) = registered(&db);
        db.set_unreachable(Some(refused()));

        session
            .db
            .execute("SELECT 1; SELECT 2", 10, CellLimit::NONE, TIMEOUT)
            .await
            .unwrap();

        assert!(stays_ok(&session, &mut events).await);
    }
}
