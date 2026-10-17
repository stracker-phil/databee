//! The UI tools: requests to the running app over the single-instance socket.

use std::path::{Path, PathBuf};

use crate::ipc::{self, IpcMessage};

pub const NOT_RUNNING: &str = "DataBee is not running";

/// The app's reply to `message`, or why there is none.
pub async fn ask(socket: &Path, message: IpcMessage) -> Result<serde_json::Value, String> {
    let socket: PathBuf = socket.to_path_buf();
    let reply = tokio::task::spawn_blocking(move || ipc::request(&socket, message))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("Could not reach DataBee: {e}"))?
        .ok_or_else(|| NOT_RUNNING.to_string())?;
    if reply.ok {
        Ok(reply.result)
    } else {
        Err(reply.error.unwrap_or_else(|| "The request failed".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[tokio::test]
    async fn without_a_listening_app_the_answer_is_that_databee_is_not_running() {
        let dir = tempdir().unwrap();

        let answer = ask(&dir.path().join("none.sock"), IpcMessage::Current).await;

        assert_eq!(answer, Err("DataBee is not running".to_string()));
    }

    #[tokio::test]
    async fn the_apps_error_is_returned_as_the_error() {
        let dir = tempdir().unwrap();
        let socket = dir.path().join("app.sock");
        let listener = ipc::bind(&socket).unwrap();
        tokio::spawn(ipc::serve(listener, |_| async {
            Err::<serde_json::Value, String>("x".into())
        }));

        let answer = ask(&socket, IpcMessage::Current).await;

        assert_eq!(answer, Err("x".to_string()));
    }

    #[tokio::test]
    async fn the_apps_result_is_returned_as_the_value() {
        let dir = tempdir().unwrap();
        let socket = dir.path().join("app.sock");
        let listener = ipc::bind(&socket).unwrap();
        tokio::spawn(ipc::serve(listener, |message| async move {
            match message {
                IpcMessage::Command { id } => Ok(json!({"ran": id})),
                _ => Err("unexpected".to_string()),
            }
        }));

        let answer = ask(
            &socket,
            IpcMessage::Command {
                id: "view.sidebar".into(),
            },
        )
        .await;

        assert_eq!(answer, Ok(json!({"ran": "view.sidebar"})));
    }
}
