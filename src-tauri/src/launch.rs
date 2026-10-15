use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;

use crate::cli::Launch;
use crate::connection::{ConnectionSpec, Secret};
use crate::ipc::IpcMessage;

/// Something the UI should act on after a (CLI) launch. Passwords stay in Rust;
/// the UI only learns whether one is available for the request.
#[derive(Serialize, Debug, Clone, PartialEq)]
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
}

#[derive(Default)]
pub struct LaunchQueue {
    events: Mutex<Vec<LaunchEvent>>,
    passwords: Mutex<HashMap<String, Secret>>,
}

impl LaunchQueue {
    /// Returns true when the message produced an event for the UI.
    pub fn push(&self, message: IpcMessage) -> bool {
        let event = match message {
            IpcMessage::Error { message } => LaunchEvent::Error { message },
            IpcMessage::Activate | IpcMessage::Eval { .. } => return false,
            launch @ IpcMessage::Launch { .. } => {
                let Some(launch) = launch.into_launch() else {
                    return false;
                };
                self.connect_event(launch)
            }
        };
        self.events.lock().unwrap().push(event);
        true
    }

    fn connect_event(&self, launch: Launch) -> LaunchEvent {
        let request_id = uuid::Uuid::new_v4().to_string();
        let has_password = launch.password.is_some();
        if let Some(password) = launch.password {
            self.passwords
                .lock()
                .unwrap()
                .insert(request_id.clone(), password);
        }
        LaunchEvent::Connect {
            request_id,
            spec: launch.spec,
            has_password,
            save_password: launch.save_password,
        }
    }

    pub fn take_events(&self) -> Vec<LaunchEvent> {
        std::mem::take(&mut *self.events.lock().unwrap())
    }

    /// Hands out a request's password once.
    pub fn take_password(&self, request_id: &str) -> Option<Secret> {
        self.passwords.lock().unwrap().remove(request_id)
    }
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
            password: password.map(String::from),
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
