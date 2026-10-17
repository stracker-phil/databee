use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use ts_rs::TS;

use crate::connection::{ConnectionSpec, Secret};
use crate::ipc::{HandlerResult, IpcMessage};
use crate::lock::MutexExt;
use crate::remote::{Bridge, RemoteRequest};
use crate::state::AppState;
use crate::window;

/// Emitted without payload when a launch arrives; the page then calls `take_launch_events`.
pub const LAUNCH_EVENT: &str = "launch";

/// A one-shot password the UI never picked up should not stay in memory for the session.
const PASSWORD_TTL: Duration = Duration::from_secs(5 * 60);

/// Something the UI should act on after a (CLI) launch. Passwords stay in Rust;
/// the UI only learns whether one is available for the request.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LaunchEvent {
    #[serde(rename_all = "camelCase")]
    Connect {
        request_id: String,
        spec: ConnectionSpec,
        has_password: bool,
        save_password: bool,
    },
    Error {
        message: String,
    },
    /// An `open` that started the app; see `IpcMessage::Open`.
    Open {
        address: String,
        connection: Option<String>,
    },
}

pub struct LaunchQueue {
    events: Mutex<Vec<LaunchEvent>>,
    passwords: Mutex<HashMap<String, (Secret, Instant)>>,
    password_ttl: Duration,
}

impl Default for LaunchQueue {
    fn default() -> Self {
        Self::with_password_ttl(PASSWORD_TTL)
    }
}

impl LaunchQueue {
    pub(crate) fn with_password_ttl(password_ttl: Duration) -> Self {
        Self {
            events: Mutex::default(),
            passwords: Mutex::default(),
            password_ttl,
        }
    }

    /// Returns true when the message produced an event for the UI.
    pub fn push(&self, message: IpcMessage) -> bool {
        self.prune();
        let event = match message {
            IpcMessage::Error { message } => LaunchEvent::Error { message },
            IpcMessage::Launch {
                spec,
                password,
                save_password,
            } => self.connect_event(spec, password, save_password),
            IpcMessage::Open {
                address,
                connection,
                ..
            } => LaunchEvent::Open {
                address,
                connection,
            },
            _ => return false,
        };
        self.events.lock_unpoisoned().push(event);
        true
    }

    fn connect_event(
        &self,
        spec: ConnectionSpec,
        password: Option<Secret>,
        save_password: bool,
    ) -> LaunchEvent {
        let request_id = uuid::Uuid::new_v4().to_string();
        let has_password = password.is_some();
        if let Some(password) = password {
            self.passwords
                .lock_unpoisoned()
                .insert(request_id.clone(), (password, Instant::now()));
        }
        LaunchEvent::Connect {
            request_id,
            spec,
            has_password,
            save_password,
        }
    }

    pub fn take_events(&self) -> Vec<LaunchEvent> {
        std::mem::take(&mut *self.events.lock_unpoisoned())
    }

    /// The request's password, kept for another attempt until `take_password`.
    pub fn peek_password(&self, request_id: &str) -> Option<Secret> {
        self.prune();
        self.passwords
            .lock_unpoisoned()
            .get(request_id)
            .map(|(secret, _)| secret.clone())
    }

    /// Hands out a request's password once.
    pub fn take_password(&self, request_id: &str) -> Option<Secret> {
        self.prune();
        self.passwords
            .lock_unpoisoned()
            .remove(request_id)
            .map(|(secret, _)| secret)
    }

    fn prune(&self) {
        let ttl = self.password_ttl;
        self.passwords
            .lock_unpoisoned()
            .retain(|_, (_, added)| added.elapsed() < ttl);
    }
}

/// Handles a message from another DataBee process (CLI launch, activation, remote request,
/// dev eval).
pub async fn on_ipc_message(handle: AppHandle, message: IpcMessage) -> HandlerResult {
    #[cfg(debug_assertions)]
    if let IpcMessage::Eval { js } = &message {
        crate::dev::eval(&handle, js);
        return Ok(serde_json::Value::Null);
    }
    log::info!("Received {} from another process", message.kind());
    let request = match message {
        IpcMessage::Open {
            address,
            connection,
            focus,
        } => {
            if focus {
                window::bring_to_front(&handle);
            }
            RemoteRequest::Open {
                address,
                connection,
            }
        }
        IpcMessage::Command { id } => RemoteRequest::Command { id },
        IpcMessage::Current => RemoteRequest::Current,
        launch => {
            queue_launches(&handle, [launch]);
            return Ok(serde_json::Value::Null);
        }
    };
    handle.state::<Bridge>().ask(&handle, request).await
}

/// Queues `messages` in order for the page, tells it, and brings the window to the front.
pub fn queue_launches(handle: &AppHandle, messages: impl IntoIterator<Item = IpcMessage>) {
    let launches = &handle.state::<AppState>().launches;
    let mut queued_event = false;
    for message in messages {
        queued_event |= launches.push(message);
    }
    if queued_event && let Err(e) = handle.emit(LAUNCH_EVENT, ()) {
        log::warn!("Couldn't notify the page of a launch request: {e}");
    }
    window::bring_to_front(handle);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch_message(password: Option<&str>) -> IpcMessage {
        IpcMessage::Launch {
            spec: ConnectionSpec {
                name: None,
                host: "h".into(),
                port: 1,
                socket: None,
                user: "u".into(),
                database: None,
            },
            password: password.map(Secret::new),
            save_password: false,
        }
    }

    fn request_id(event: &LaunchEvent) -> String {
        match event {
            LaunchEvent::Connect { request_id, .. } => request_id.clone(),
            _ => panic!("expected connect event"),
        }
    }

    #[test]
    fn events_are_taken_once() {
        let queue = LaunchQueue::default();
        queue.push(launch_message(None));
        assert_eq!(queue.take_events().len(), 1);
        assert!(queue.take_events().is_empty());
    }

    #[test]
    fn a_launch_pushed_before_an_open_is_taken_before_it() {
        let queue = LaunchQueue::default();
        queue.push(launch_message(None));
        queue.push(IpcMessage::Open {
            address: "databee://local/wp/".into(),
            connection: None,
            focus: true,
        });

        let events = queue.take_events();

        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], LaunchEvent::Connect { .. }));
        assert!(matches!(
            &events[1],
            LaunchEvent::Open { address, connection: None } if address == "databee://local/wp/"
        ));
    }

    #[test]
    fn password_is_not_exposed_in_event_but_retrievable_once() {
        let queue = LaunchQueue::default();
        queue.push(launch_message(Some("pw")));
        let events = queue.take_events();
        let json = serde_json::to_string(&events).unwrap();
        assert!(!json.contains("\"pw\""));
        assert!(json.contains("\"hasPassword\":true"));

        let id = request_id(&events[0]);
        assert_eq!(queue.take_password(&id), Some(Secret::new("pw")));
        assert_eq!(queue.take_password(&id), None);
    }

    #[test]
    fn activation_alone_creates_no_event() {
        let queue = LaunchQueue::default();
        assert!(!queue.push(IpcMessage::Activate));
        assert!(queue.take_events().is_empty());
    }

    #[cfg(debug_assertions)]
    #[test]
    fn eval_messages_create_no_event() {
        let queue = LaunchQueue::default();
        assert!(!queue.push(IpcMessage::Eval { js: "1".into() }));
        assert!(queue.take_events().is_empty());
    }

    #[test]
    fn launch_without_password_reports_none_available() {
        let queue = LaunchQueue::default();
        assert!(queue.push(launch_message(None)));
        let events = queue.take_events();
        assert!(
            serde_json::to_string(&events)
                .unwrap()
                .contains("\"hasPassword\":false")
        );
        let id = request_id(&events[0]);
        assert_eq!(queue.peek_password(&id), None);
        assert_eq!(queue.take_password(&id), None);
    }

    #[test]
    fn each_launch_gets_its_own_request_id() {
        let queue = LaunchQueue::default();
        queue.push(launch_message(None));
        queue.push(launch_message(None));
        let events = queue.take_events();
        assert_ne!(request_id(&events[0]), request_id(&events[1]));
    }

    #[test]
    fn save_password_flag_is_carried_into_the_event() {
        let queue = LaunchQueue::default();
        let IpcMessage::Launch { spec, .. } = launch_message(None) else {
            unreachable!()
        };
        queue.push(IpcMessage::Launch {
            spec,
            password: None,
            save_password: true,
        });
        assert!(matches!(
            queue.take_events()[0],
            LaunchEvent::Connect {
                save_password: true,
                ..
            }
        ));
    }

    #[test]
    fn peeked_password_stays_until_taken() {
        let queue = LaunchQueue::default();
        queue.push(launch_message(Some("pw")));
        let id = request_id(&queue.take_events()[0]);
        assert_eq!(queue.peek_password(&id), Some(Secret::new("pw")));
        assert_eq!(queue.peek_password(&id), Some(Secret::new("pw")));
        assert_eq!(queue.take_password(&id), Some(Secret::new("pw")));
        assert_eq!(queue.peek_password(&id), None);
    }

    #[test]
    fn expired_password_is_gone() {
        let queue = LaunchQueue::with_password_ttl(Duration::ZERO);
        queue.push(launch_message(Some("pw")));
        let id = request_id(&queue.take_events()[0]);
        assert_eq!(queue.peek_password(&id), None);
        assert_eq!(queue.take_password(&id), None);
    }

    #[test]
    fn open_that_started_the_app_is_queued_without_its_focus_flag() {
        let queue = LaunchQueue::default();
        let queued = queue.push(IpcMessage::Open {
            address: "order 4711".into(),
            connection: Some("shop".into()),
            focus: true,
        });
        assert!(queued);
        assert_eq!(
            queue.take_events(),
            [LaunchEvent::Open {
                address: "order 4711".into(),
                connection: Some("shop".into()),
            }]
        );
    }

    #[test]
    fn command_and_current_requests_queue_nothing() {
        let queue = LaunchQueue::default();
        for message in [
            IpcMessage::Command {
                id: "view.sidebar".into(),
            },
            IpcMessage::Current,
        ] {
            assert!(!queue.push(message.clone()), "{message:?}");
        }
        assert!(queue.take_events().is_empty());
    }

    #[test]
    fn cli_errors_are_reported_to_the_ui() {
        let queue = LaunchQueue::default();
        queue.push(IpcMessage::Error {
            message: "bad flag".into(),
        });
        assert_eq!(
            queue.take_events(),
            [LaunchEvent::Error {
                message: "bad flag".into()
            }]
        );
    }
}
