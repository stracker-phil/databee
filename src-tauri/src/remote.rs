//! Requests from other processes that only the page can answer (open an address, run a command,
//! the active tab's address): emitted as `REMOTE_EVENT` with an id, answered by `remote_reply`.

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use ts_rs::TS;

use crate::ipc::HandlerResult;
use crate::pending::{NoReply, PendingReplies};

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
    pending: PendingReplies<HandlerResult>,
}

impl Bridge {
    /// Asks the page and waits for its answer, at most `REPLY_TIMEOUT`.
    pub async fn ask(&self, handle: &AppHandle, request: RemoteRequest) -> HandlerResult {
        let sent = self.pending.ask(REPLY_TIMEOUT, |id| {
            let event = RemoteEvent {
                id: id.to_string(),
                request,
            };
            handle.emit(REMOTE_EVENT, event)
        });
        match sent.await {
            Ok(result) => result,
            Err(NoReply::NotSent(e)) => Err(format!("Couldn't reach the DataBee window: {e}")),
            Err(NoReply::Dropped) => Err("The DataBee window dropped the request".into()),
            Err(NoReply::TimedOut) => Err("The DataBee window did not answer in time".into()),
        }
    }

    /// False when no request with `id` waits (answered, timed out or unknown).
    pub fn answer(&self, id: &str, result: HandlerResult) -> bool {
        self.pending.answer(id, result)
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
