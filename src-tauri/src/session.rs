use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde::Serialize;

use crate::connection::ConnectionSpec;
use crate::db::mysql::MySqlSession;
use crate::error::{AppError, AppResult};

pub struct Session {
    pub id: String,
    pub spec: ConnectionSpec,
    pub db: MySqlSession,
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
            server_version: self.db.server_version.clone(),
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
            .write()
            .unwrap()
            .insert(session.id.clone(), session.clone());
        session
    }

    pub fn get(&self, id: &str) -> AppResult<Arc<Session>> {
        self.0
            .read()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::SessionNotFound(id.to_string()))
    }

    pub fn remove(&self, id: &str) -> Option<Arc<Session>> {
        self.0.write().unwrap().remove(id)
    }
}
