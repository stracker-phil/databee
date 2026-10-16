use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde::Serialize;

use crate::connection::ConnectionSpec;
use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::lock::RwLockExt;

pub struct Session {
    pub id: String,
    pub spec: ConnectionSpec,
    pub db: Box<dyn Database>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub spec: ConnectionSpec,
    pub identity: String,
    pub display_name: String,
    pub server_version: String,
    pub database: Option<String>,
}

impl Session {
    pub fn info(&self) -> SessionInfo {
        SessionInfo {
            id: self.id.clone(),
            spec: self.spec.clone(),
            identity: self.spec.identity(),
            display_name: self.spec.display_name(),
            server_version: self.db.server_version().to_string(),
            database: self.db.current_database(),
        }
    }
}

#[derive(Default)]
pub struct Sessions(RwLock<HashMap<String, Arc<Session>>>);

impl Sessions {
    pub fn insert(&self, session: Session) -> Arc<Session> {
        let session = Arc::new(session);
        self.0
            .write_unpoisoned()
            .insert(session.id.clone(), session.clone());
        session
    }

    pub fn get(&self, id: &str) -> AppResult<Arc<Session>> {
        self.0
            .read_unpoisoned()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::SessionNotFound(id.to_string()))
    }

    pub fn remove(&self, id: &str) -> Option<Arc<Session>> {
        self.0.write_unpoisoned().remove(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::FakeDatabase;

    fn session(id: &str) -> Session {
        Session {
            id: id.into(),
            spec: ConnectionSpec {
                name: None,
                host: "127.0.0.1".into(),
                port: 3306,
                socket: None,
                user: "root".into(),
                database: None,
            },
            db: Box::new(FakeDatabase::new().with_database("wp")),
        }
    }

    #[test]
    fn unknown_session_id_is_a_session_not_found_error() {
        let err = Sessions::default().get("missing").err().unwrap();

        assert_eq!(err.kind(), "session_not_found");
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
        assert_eq!(sessions.get("a").err().unwrap().kind(), "session_not_found");
    }

    #[test]
    fn info_reports_server_version_and_current_database() {
        let info = session("a").info();

        assert_eq!(info.id, "a");
        assert_eq!(info.server_version, "fake");
        assert_eq!(info.database.as_deref(), Some("wp"));
    }
}
