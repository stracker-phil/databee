//! The app's health tasks: one tick a second starts the pings and retries that are due, and
//! every health change goes to the page as a `session_health` event.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::broadcast::error::RecvError;
use tokio::time::MissedTickBehavior;

use super::{Due, HealthEvent, check, retry};
use crate::session::Session;
use crate::state::AppState;

pub const EVENT: &str = "session_health";
const TICK: Duration = Duration::from_secs(1);

pub fn start(app: &AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut events = handle.state::<AppState>().sessions.health_events();
        loop {
            match events.recv().await {
                Ok(event) => emit(&handle, &event),
                // Missed changes are made good by sending where every session stands now.
                Err(RecvError::Lagged(_)) => {
                    for session in handle.state::<AppState>().sessions.all() {
                        let event = HealthEvent {
                            session_id: session.id.clone(),
                            health: session.health.get(),
                        };
                        emit(&handle, &event);
                    }
                }
                Err(RecvError::Closed) => break,
            }
        }
    });
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut ticks = tokio::time::interval(TICK);
        ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            ticks.tick().await;
            for session in handle.state::<AppState>().sessions.all() {
                tend(session, Instant::now());
            }
        }
    });
}

fn emit(app: &AppHandle, event: &HealthEvent) {
    if let Err(e) = app.emit(EVENT, event) {
        log::warn!("Could not send the session health to the page: {e}");
    }
}

/// Starts what `session` is due for, in its own task so a slow server holds up no other session.
pub fn tend(session: Arc<Session>, now: Instant) {
    match session.health.due(now) {
        Due::Check => {
            tauri::async_runtime::spawn(async move {
                check(session.db.as_ref(), &session.health).await;
            });
        }
        Due::Retry => {
            tauri::async_runtime::spawn(async move {
                retry(session.db.as_ref(), &session.health).await;
            });
        }
        Due::Nothing => {}
    }
}
