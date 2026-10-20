use tauri::State;

use super::{Health, retry};
use crate::error::AppResult;
use crate::session::Sessions;
use crate::state::AppState;

/// Retry (the toolbar's button, and once when the window regains focus): reconnects a `down`
/// session with the password it already holds, so nothing asks for one. Returns where the
/// session stands; an `ok` or already reconnecting session is left alone.
#[tauri::command]
pub async fn reconnect_session(
    state: State<'_, AppState>,
    session_id: String,
) -> AppResult<Health> {
    reconnect(&state.sessions, &session_id).await
}

async fn reconnect(sessions: &Sessions, session_id: &str) -> AppResult<Health> {
    let session = sessions.get(session_id)?;
    Ok(retry(session.db.as_ref(), &session.health).await)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::db::FakeDatabase;
    use crate::error::{AppError, ErrorKind};
    use crate::health::tests::{go_down, spec};
    use crate::session::Session;

    fn sessions_with(db: &Arc<FakeDatabase>) -> Sessions {
        let sessions = Sessions::default();
        sessions.insert(Session::new(
            "a".into(),
            spec(),
            "local".into(),
            Box::new(db.clone()),
        ));
        sessions
    }

    #[tokio::test]
    async fn reconnecting_an_unknown_session_is_a_session_not_found_error() {
        let err = reconnect(&Sessions::default(), "missing")
            .await
            .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::SessionNotFound);
    }

    #[tokio::test]
    async fn reconnecting_a_down_session_returns_ok_once_the_server_answers() {
        let db = Arc::new(FakeDatabase::new());
        let sessions = sessions_with(&db);
        go_down(&db, &sessions.get("a").unwrap()).await;
        db.set_unreachable(None);

        let health = reconnect(&sessions, "a").await.unwrap();

        assert_eq!(health, Health::Ok);
        assert_eq!(sessions.get("a").unwrap().health.get(), Health::Ok);
    }

    #[tokio::test]
    async fn reconnecting_a_down_session_whose_server_is_still_gone_returns_down() {
        let db = Arc::new(FakeDatabase::new());
        let sessions = sessions_with(&db);
        go_down(&db, &sessions.get("a").unwrap()).await;
        db.set_unreachable(Some(AppError::Connection("still down".into())));

        let health = reconnect(&sessions, "a").await.unwrap();

        let Health::Down { error, .. } = health else {
            panic!("not down: {health:?}");
        };
        assert_eq!(error, "Could not connect: still down");
    }

    #[tokio::test]
    async fn reconnecting_an_ok_session_returns_ok() {
        let db = Arc::new(FakeDatabase::new());
        let sessions = sessions_with(&db);

        assert_eq!(reconnect(&sessions, "a").await.unwrap(), Health::Ok);
    }
}
