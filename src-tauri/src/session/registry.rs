use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use tokio::sync::broadcast;

use super::Session;
use crate::error::{AppError, AppResult};
use crate::health::HealthEvent;
use crate::lock::RwLockExt;

/// Health changes queued for the page; a longer burst makes the receiver resend every state.
const HEALTH_EVENTS: usize = 64;

pub struct Sessions {
    open: RwLock<HashMap<String, Arc<Session>>>,
    /// The health changes of every registered session (`health::watch` sends them to the page).
    health: broadcast::Sender<HealthEvent>,
}

impl Default for Sessions {
    fn default() -> Self {
        Self {
            open: RwLock::default(),
            health: broadcast::channel(HEALTH_EVENTS).0,
        }
    }
}

impl Sessions {
    /// Registers the session; from now on its health changes are reported.
    pub fn insert(&self, session: Session) -> Arc<Session> {
        session.health.attach(self.health.clone());
        let session = Arc::new(session);
        self.open
            .write_unpoisoned()
            .insert(session.id.clone(), session.clone());
        session
    }

    pub fn get(&self, id: &str) -> AppResult<Arc<Session>> {
        self.open
            .read_unpoisoned()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::SessionNotFound(id.to_string()))
    }

    pub fn all(&self) -> Vec<Arc<Session>> {
        self.open.read_unpoisoned().values().cloned().collect()
    }

    pub fn remove(&self, id: &str) -> Option<Arc<Session>> {
        self.open.write_unpoisoned().remove(id)
    }

    pub fn health_events(&self) -> broadcast::Receiver<HealthEvent> {
        self.health.subscribe()
    }

    pub fn has_slug(&self, slug: &str) -> bool {
        self.open
            .read_unpoisoned()
            .values()
            .any(|s| s.slug() == slug)
    }

    /// Gives every open session of the connection `identity` the renamed slug.
    pub fn rename_slug(&self, identity: &str, slug: &str) {
        for session in self.open.read_unpoisoned().values() {
            if session.spec.identity() == identity {
                session.set_slug(slug);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::FakeDatabase;
    use crate::error::ErrorKind;
    use crate::session::testing::{local_spec, session, socket_spec};

    #[test]
    fn unknown_session_id_is_a_session_not_found_error() {
        let err = Sessions::default().get("missing").err().unwrap();

        assert_eq!(err.kind(), ErrorKind::SessionNotFound);
    }

    #[test]
    fn get_returns_the_inserted_session() {
        let sessions = Sessions::default();
        let inserted = sessions.insert(session("a"));

        let found = sessions.get("a").unwrap();

        assert!(Arc::ptr_eq(&inserted, &found));
    }

    #[test]
    fn remove_returns_the_session_once() {
        let sessions = Sessions::default();
        sessions.insert(session("a"));

        assert!(sessions.remove("a").is_some());
        assert!(sessions.remove("a").is_none());
        assert_eq!(
            sessions.get("a").err().unwrap().kind(),
            ErrorKind::SessionNotFound
        );
    }

    #[test]
    fn renaming_a_slug_changes_only_the_sessions_of_that_connection() {
        let sessions = Sessions::default();
        let first = sessions.insert(session("a"));
        let second = sessions.insert(session("b"));
        let other = sessions.insert(Session::new(
            "c".into(),
            socket_spec(),
            "socket".into(),
            Box::new(FakeDatabase::new()),
        ));

        sessions.rename_slug(&local_spec().identity(), "renamed");

        assert_eq!(first.slug(), "renamed");
        assert_eq!(second.slug(), "renamed");
        assert_eq!(other.slug(), "socket");
    }

    #[test]
    fn has_slug_is_true_only_for_the_slug_of_an_open_session() {
        let sessions = Sessions::default();
        assert!(!sessions.has_slug("local"));

        sessions.insert(session("a"));

        assert!(sessions.has_slug("local"));
        assert!(!sessions.has_slug("other"));
    }

    #[test]
    fn has_slug_follows_a_renamed_slug() {
        let sessions = Sessions::default();
        sessions.insert(session("a"));

        sessions.rename_slug(&local_spec().identity(), "renamed");

        assert!(sessions.has_slug("renamed"));
        assert!(!sessions.has_slug("local"));
    }
}
