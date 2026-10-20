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
    Reconnecting { since: i64, error: String },
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
        log::warn!("Lost the connection of session {}: {error}", self.session_id);
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
