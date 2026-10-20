//! Whether each session's server still answers. A session is `ok`, `down` (since when, the last
//! error, the next automatic retry) or `reconnecting`. Pings every `PING_INTERVAL`, an error that
//! suggests a lost connection, and retries move it between them; every change is sent to the
//! page as a `session_health` event.

pub mod commands;
mod guard;
pub mod watch;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::broadcast;
use ts_rs::TS;

pub use guard::Guarded;

use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::lock::MutexExt;

/// How often an `ok` session is pinged.
pub const PING_INTERVAL: Duration = Duration::from_secs(10);
/// The wait before each automatic retry of a `down` session; the last step repeats.
pub const RETRY_BACKOFF: [Duration; 3] = [
    Duration::from_secs(10),
    Duration::from_secs(30),
    Duration::from_secs(60),
];

/// Times are Unix milliseconds.
#[derive(Serialize, Clone, Debug, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Health {
    Ok,
    #[serde(rename_all = "camelCase")]
    Down {
        since: i64,
        error: String,
        /// When the next automatic retry starts.
        retry_at: i64,
    },
    #[serde(rename_all = "camelCase")]
    Reconnecting {
        since: i64,
        error: String,
    },
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct HealthEvent {
    pub session_id: String,
    pub health: Health,
}

/// What the watcher should start for a session now.
#[derive(Debug, PartialEq, Eq)]
pub enum Due {
    Check,
    Retry,
    Nothing,
}

pub struct SessionHealth {
    session_id: String,
    inner: Mutex<Inner>,
    checking: AtomicBool,
    /// Set when the session is registered (`Sessions::insert`); only then does it refuse calls
    /// while down, because only then does something retry it.
    events: OnceLock<broadcast::Sender<HealthEvent>>,
}

struct Inner {
    health: Health,
    last_check: Instant,
    /// Failed retries since the session went down; picks the next `RETRY_BACKOFF` step.
    failed_retries: usize,
    next_retry: Instant,
}

fn unix_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

impl SessionHealth {
    pub fn new(session_id: &str) -> Self {
        let now = Instant::now();
        Self {
            session_id: session_id.to_string(),
            inner: Mutex::new(Inner {
                health: Health::Ok,
                last_check: now,
                failed_retries: 0,
                next_retry: now,
            }),
            checking: AtomicBool::new(false),
            events: OnceLock::new(),
        }
    }

    pub fn attach(&self, events: broadcast::Sender<HealthEvent>) {
        let _ = self.events.set(events);
    }

    pub fn get(&self) -> Health {
        self.inner.lock_unpoisoned().health.clone()
    }

    /// The error a call that needs the server gets instead, while a registered session is not `ok`.
    pub fn refusal(&self) -> Option<AppError> {
        self.events.get()?;
        match &self.inner.lock_unpoisoned().health {
            Health::Ok => None,
            Health::Down { error, .. } | Health::Reconnecting { error, .. } => {
                Some(AppError::ConnectionLost(error.clone()))
            }
        }
    }

    pub fn due(&self, now: Instant) -> Due {
        let inner = self.inner.lock_unpoisoned();
        match inner.health {
            Health::Ok
                if !self.checking.load(Ordering::SeqCst)
                    && now.saturating_duration_since(inner.last_check) >= PING_INTERVAL =>
            {
                Due::Check
            }
            Health::Down { .. } if now >= inner.next_retry => Due::Retry,
            _ => Due::Nothing,
        }
    }

    fn set(&self, inner: &mut Inner, health: Health) {
        if inner.health == health {
            return;
        }
        inner.health = health.clone();
        if let Some(events) = self.events.get() {
            // No receiver only means no window listens yet.
            let _ = events.send(HealthEvent {
                session_id: self.session_id.clone(),
                health,
            });
        }
    }

    fn mark_down(&self, inner: &mut Inner, since: i64, error: &AppError, now: Instant) {
        let wait = RETRY_BACKOFF[inner.failed_retries.min(RETRY_BACKOFF.len() - 1)];
        inner.next_retry = now + wait;
        let health = Health::Down {
            since,
            error: error.to_string(),
            retry_at: unix_ms() + wait.as_millis() as i64,
        };
        self.set(inner, health);
    }

    fn begin_check(&self, now: Instant) -> Option<CheckGuard<'_>> {
        let mut inner = self.inner.lock_unpoisoned();
        if inner.health != Health::Ok || self.checking.swap(true, Ordering::SeqCst) {
            return None;
        }
        inner.last_check = now;
        Some(CheckGuard(&self.checking))
    }

    fn end_check(&self, result: AppResult<()>, now: Instant) {
        let Err(error) = result else {
            return;
        };
        let mut inner = self.inner.lock_unpoisoned();
        if inner.health != Health::Ok {
            return;
        }
        log::warn!(
            "Lost the connection of session {}: {error}",
            self.session_id
        );
        inner.failed_retries = 0;
        self.mark_down(&mut inner, unix_ms(), &error, now);
    }

    fn begin_retry(&self) -> Option<Attempt<'_>> {
        let mut inner = self.inner.lock_unpoisoned();
        let Health::Down { since, error, .. } = &inner.health else {
            return None;
        };
        let reconnecting = Health::Reconnecting {
            since: *since,
            error: error.clone(),
        };
        self.set(&mut inner, reconnecting);
        Some(Attempt {
            health: self,
            finished: false,
        })
    }

    fn end_retry(&self, result: AppResult<()>, now: Instant) -> Health {
        let mut inner = self.inner.lock_unpoisoned();
        let Health::Reconnecting { since, .. } = inner.health else {
            return inner.health.clone();
        };
        match result {
            Ok(()) => {
                log::info!("Reconnected session {}", self.session_id);
                inner.failed_retries = 0;
                inner.last_check = now;
                self.set(&mut inner, Health::Ok);
            }
            Err(error) => {
                inner.failed_retries += 1;
                self.mark_down(&mut inner, since, &error, now);
            }
        }
        inner.health.clone()
    }
}

/// Clears the in-flight flag however the check ends.
struct CheckGuard<'a>(&'a AtomicBool);

impl Drop for CheckGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// A retry that is dropped before it finishes counts as failed, so the session is retried again.
struct Attempt<'a> {
    health: &'a SessionHealth,
    finished: bool,
}

impl Attempt<'_> {
    fn finish(mut self, result: AppResult<()>) -> Health {
        self.finished = true;
        self.health.end_retry(result, Instant::now())
    }
}

impl Drop for Attempt<'_> {
    fn drop(&mut self) {
        if !self.finished {
            let interrupted = AppError::Connection("the reconnect was interrupted".into());
            self.health.end_retry(Err(interrupted), Instant::now());
        }
    }
}

/// Pings an `ok` session, one check at a time; a failed ping marks it `down`.
pub async fn check(db: &dyn Database, health: &SessionHealth) {
    let Some(_checking) = health.begin_check(Instant::now()) else {
        return;
    };
    let result = db.ping().await;
    health.end_check(result, Instant::now());
}

/// Reconnects a `down` session with the credentials it holds and returns where it stands; a
/// session that is `ok` or already reconnecting is left alone.
pub async fn retry(db: &dyn Database, health: &SessionHealth) -> Health {
    let Some(attempt) = health.begin_retry() else {
        return health.get();
    };
    let result = db.reconnect().await;
    attempt.finish(result)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use tokio::sync::broadcast::Receiver;
    use tokio::sync::broadcast::error::TryRecvError;

    use super::*;
    use crate::connection::ConnectionSpec;
    use crate::db::FakeDatabase;
    use crate::session::{Session, Sessions};

    const REFUSED: &str = "Could not connect: refused";

    pub(super) fn refused() -> AppError {
        AppError::Connection("refused".into())
    }

    pub(super) fn spec() -> ConnectionSpec {
        ConnectionSpec {
            name: None,
            host: "127.0.0.1".into(),
            port: 3306,
            socket: None,
            user: "root".into(),
            database: None,
        }
    }

    pub(super) fn registered(db: &Arc<FakeDatabase>) -> (Arc<Session>, Receiver<HealthEvent>) {
        let sessions = Sessions::default();
        let events = sessions.health_events();
        let session = sessions.insert(Session::new(
            "a".into(),
            spec(),
            "local".into(),
            Box::new(db.clone()),
        ));
        (session, events)
    }

    pub(super) fn drain(events: &mut Receiver<HealthEvent>) -> Vec<Health> {
        let mut out = Vec::new();
        while let Ok(event) = events.try_recv() {
            assert_eq!(event.session_id, "a");
            out.push(event.health);
        }
        out
    }

    pub(super) async fn go_down(db: &FakeDatabase, session: &Session) -> Health {
        db.set_unreachable(Some(refused()));
        check(session.db.as_ref(), &session.health).await;
        session.health.get()
    }

    fn assert_retries_in(health: &Health, window: (i64, i64), secs: i64) {
        let Health::Down { retry_at, .. } = health else {
            panic!("not down: {health:?}");
        };
        let wait = secs * 1000;
        assert!(
            (window.0 + wait..=window.1 + wait).contains(retry_at),
            "{secs} s: {retry_at} is outside {window:?}"
        );
    }

    #[tokio::test]
    async fn failed_ping_turns_ok_into_down_and_sends_one_event() {
        let db = Arc::new(FakeDatabase::new());
        let (session, mut events) = registered(&db);
        db.set_unreachable(Some(refused()));
        let before = unix_ms();

        check(session.db.as_ref(), &session.health).await;

        let after = unix_ms();
        let health = session.health.get();
        let Health::Down { since, error, .. } = &health else {
            panic!("not down: {health:?}");
        };
        assert!((before..=after).contains(since), "{since}");
        assert_eq!(error, REFUSED);
        assert_retries_in(&health, (before, after), 10);
        assert_eq!(drain(&mut events), [health]);
    }

    #[tokio::test]
    async fn successful_ping_changes_nothing_and_sends_nothing() {
        let db = Arc::new(FakeDatabase::new());
        let (session, mut events) = registered(&db);

        check(session.db.as_ref(), &session.health).await;

        assert_eq!(session.health.get(), Health::Ok);
        assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    }

    #[tokio::test]
    async fn check_leaves_a_down_session_as_it_is() {
        let db = Arc::new(FakeDatabase::new());
        let (session, mut events) = registered(&db);
        let down = go_down(&db, &session).await;
        drain(&mut events);
        db.set_unreachable(None);

        check(session.db.as_ref(), &session.health).await;

        assert_eq!(session.health.get(), down);
        assert!(drain(&mut events).is_empty());
    }

    #[tokio::test]
    async fn an_ok_session_is_checked_once_the_ping_interval_has_passed() {
        let (session, _events) = registered(&Arc::new(FakeDatabase::new()));
        let now = Instant::now();

        assert_eq!(session.health.due(now), Due::Nothing);
        assert_eq!(
            session
                .health
                .due(now + PING_INTERVAL - Duration::from_secs(5)),
            Due::Nothing
        );
        assert_eq!(session.health.due(now + PING_INTERVAL), Due::Check);
    }

    #[tokio::test]
    async fn no_check_is_due_while_one_is_in_flight() {
        let (session, _events) = registered(&Arc::new(FakeDatabase::new()));
        let later = Instant::now() + PING_INTERVAL * 2;

        let in_flight = session.health.begin_check(Instant::now()).unwrap();
        assert_eq!(session.health.due(later), Due::Nothing);
        assert!(session.health.begin_check(Instant::now()).is_none());

        drop(in_flight);
        assert_eq!(session.health.due(later), Due::Check);
    }

    #[tokio::test]
    async fn a_down_session_is_retried_only_once_its_backoff_has_passed() {
        let db = Arc::new(FakeDatabase::new());
        let (session, _events) = registered(&db);
        go_down(&db, &session).await;
        let now = Instant::now();

        assert_eq!(session.health.due(now), Due::Nothing);
        assert_eq!(
            session.health.due(now + Duration::from_secs(9)),
            Due::Nothing
        );
        assert_eq!(
            session.health.due(now + Duration::from_secs(11)),
            Due::Retry
        );
    }

    #[tokio::test]
    async fn a_reconnecting_session_is_neither_checked_nor_retried() {
        let db = Arc::new(FakeDatabase::new());
        let (session, _events) = registered(&db);
        go_down(&db, &session).await;
        let far = Instant::now() + Duration::from_secs(3600);

        let attempt = session.health.begin_retry().unwrap();

        assert!(matches!(session.health.get(), Health::Reconnecting { .. }));
        assert_eq!(session.health.due(far), Due::Nothing);
        drop(attempt);
    }

    #[tokio::test]
    async fn successful_retry_goes_reconnecting_then_ok() {
        let db = Arc::new(FakeDatabase::new());
        let (session, mut events) = registered(&db);
        let Health::Down { since, error, .. } = go_down(&db, &session).await else {
            panic!("not down");
        };
        drain(&mut events);
        db.set_unreachable(None);

        let health = retry(session.db.as_ref(), &session.health).await;

        assert_eq!(health, Health::Ok);
        assert_eq!(session.health.get(), Health::Ok);
        assert_eq!(
            drain(&mut events),
            [Health::Reconnecting { since, error }, Health::Ok]
        );
    }

    #[tokio::test]
    async fn failing_retry_goes_reconnecting_then_down_with_the_new_error_and_the_same_since() {
        let db = Arc::new(FakeDatabase::new());
        let (session, mut events) = registered(&db);
        let Health::Down { since, error, .. } = go_down(&db, &session).await else {
            panic!("not down");
        };
        drain(&mut events);
        db.set_unreachable(Some(AppError::Connection("still down".into())));

        let health = retry(session.db.as_ref(), &session.health).await;

        let Health::Down {
            since: new_since,
            error: new_error,
            ..
        } = &health
        else {
            panic!("not down: {health:?}");
        };
        assert_eq!(*new_since, since);
        assert_eq!(new_error, "Could not connect: still down");
        assert_eq!(
            drain(&mut events),
            [Health::Reconnecting { since, error }, health]
        );
    }

    #[tokio::test]
    async fn retries_back_off_from_10_to_30_to_60_seconds_and_stay_at_60() {
        let db = Arc::new(FakeDatabase::new());
        let (session, _events) = registered(&db);
        let before = unix_ms();
        let health = go_down(&db, &session).await;
        assert_retries_in(&health, (before, unix_ms()), 10);

        for secs in [30, 60, 60, 60] {
            let before = unix_ms();
            let health = retry(session.db.as_ref(), &session.health).await;
            assert_retries_in(&health, (before, unix_ms()), secs);
        }
    }

    #[tokio::test]
    async fn a_failed_retry_postpones_the_next_one_by_the_next_backoff_step() {
        let db = Arc::new(FakeDatabase::new());
        let (session, _events) = registered(&db);
        go_down(&db, &session).await;
        retry(session.db.as_ref(), &session.health).await;
        let now = Instant::now();

        assert_eq!(
            session.health.due(now + Duration::from_secs(29)),
            Due::Nothing
        );
        assert_eq!(
            session.health.due(now + Duration::from_secs(31)),
            Due::Retry
        );
    }

    #[tokio::test]
    async fn a_new_loss_after_a_reconnect_starts_the_backoff_over() {
        let db = Arc::new(FakeDatabase::new());
        let (session, _events) = registered(&db);
        go_down(&db, &session).await;
        retry(session.db.as_ref(), &session.health).await;
        db.set_unreachable(None);
        retry(session.db.as_ref(), &session.health).await;

        let before = unix_ms();
        let health = go_down(&db, &session).await;

        assert_retries_in(&health, (before, unix_ms()), 10);
    }

    #[tokio::test]
    async fn retry_on_an_ok_session_returns_ok_and_sends_nothing() {
        let db = Arc::new(FakeDatabase::new());
        let (session, mut events) = registered(&db);
        db.set_unreachable(Some(refused()));

        let health = retry(session.db.as_ref(), &session.health).await;

        assert_eq!(health, Health::Ok);
        assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    }

    #[tokio::test]
    async fn a_retry_dropped_before_it_finishes_counts_as_failed() {
        let db = Arc::new(FakeDatabase::new());
        let (session, _events) = registered(&db);
        let Health::Down { since, .. } = go_down(&db, &session).await else {
            panic!("not down");
        };

        drop(session.health.begin_retry().unwrap());

        let health = session.health.get();
        let Health::Down {
            since: new_since,
            error,
            ..
        } = &health
        else {
            panic!("not down: {health:?}");
        };
        assert_eq!(*new_since, since);
        assert_eq!(error, "Could not connect: the reconnect was interrupted");
    }

    #[test]
    fn health_serializes_its_state_as_a_tag_with_camel_case_fields() {
        for (health, expected) in [
            (Health::Ok, json!({"state": "ok"})),
            (
                Health::Down {
                    since: 1_000,
                    error: "Could not connect: refused".into(),
                    retry_at: 11_000,
                },
                json!({
                    "state": "down",
                    "since": 1_000,
                    "error": "Could not connect: refused",
                    "retryAt": 11_000
                }),
            ),
            (
                Health::Reconnecting {
                    since: 1_000,
                    error: "Could not connect: refused".into(),
                },
                json!({
                    "state": "reconnecting",
                    "since": 1_000,
                    "error": "Could not connect: refused"
                }),
            ),
        ] {
            assert_eq!(
                serde_json::to_value(&health).unwrap(),
                expected,
                "{health:?}"
            );
        }
    }

    #[test]
    fn health_event_serializes_the_session_id_in_camel_case() {
        let event = HealthEvent {
            session_id: "a".into(),
            health: Health::Down {
                since: 1_000,
                error: "gone".into(),
                retry_at: 11_000,
            },
        };

        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            json!({
                "sessionId": "a",
                "health": {"state": "down", "since": 1_000, "error": "gone", "retryAt": 11_000}
            })
        );
    }
}
