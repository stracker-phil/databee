//! Requests from other processes that only the page can answer (open an address, run a command,
//! the active tab's address): emitted as `REMOTE_EVENT` with an id, answered by `remote_reply`.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::oneshot;
use ts_rs::TS;

use crate::ipc::HandlerResult;
use crate::lock::MutexExt;

pub const REMOTE_EVENT: &str = "remote";

/// Below the IPC client's timeout, so the sender still reads the reply.
const REPLY_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RemoteRequest {
    /// `address` is an address, or an entity shorthand (`order 4711`) in the session of
    /// `connection` (the active session when `None`).
    Open {
        address: String,
        connection: Option<String>,
    },
    Command {
        id: String,
    },
    Current,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct RemoteEvent {
    pub id: String,
    pub request: RemoteRequest,
}

#[derive(Default)]
pub struct Bridge {
    pending: Mutex<HashMap<String, oneshot::Sender<HandlerResult>>>,
}

impl Bridge {
    /// Asks the page and waits for its answer, at most `REPLY_TIMEOUT`.
    pub async fn ask(&self, handle: &AppHandle, request: RemoteRequest) -> HandlerResult {
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending.lock_unpoisoned().insert(id.clone(), tx);
        let event = RemoteEvent {
            id: id.clone(),
            request,
        };
        if let Err(e) = handle.emit(REMOTE_EVENT, event) {
            self.pending.lock_unpoisoned().remove(&id);
            return Err(format!("Couldn't reach the DataBee window: {e}"));
        }
        let answer = tokio::time::timeout(REPLY_TIMEOUT, rx).await;
        self.pending.lock_unpoisoned().remove(&id);
        match answer {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("The DataBee window dropped the request".into()),
            Err(_) => Err("The DataBee window did not answer in time".into()),
        }
    }

    /// False when no request with `id` waits (answered, timed out or unknown).
    pub fn answer(&self, id: &str, result: HandlerResult) -> bool {
        let Some(tx) = self.pending.lock_unpoisoned().remove(id) else {
            return false;
        };
        tx.send(result).is_ok()
    }
}

/// The page's answer to a `REMOTE_EVENT`: `error` set means it failed.
#[tauri::command]
pub fn remote_reply(
    bridge: State<'_, Bridge>,
    id: String,
    result: Option<serde_json::Value>,
    error: Option<String>,
) {
    let outcome = match error {
        Some(error) => Err(error),
        None => Ok(result.unwrap_or_default()),
    };
    if !bridge.answer(&id, outcome) {
        log::debug!("Late or unknown remote reply {id}");
    }
}
