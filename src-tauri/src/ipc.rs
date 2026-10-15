//! Single-instance handoff: a second `databee` process sends its launch request over a
//! Unix socket (mode 0600) to the running app and exits. Passwords stay on this machine
//! and never appear in argv of the running app.

use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

use crate::cli::Launch;
use crate::connection::{ConnectionSpec, Secret};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum IpcMessage {
    #[serde(rename_all = "camelCase")]
    Launch {
        spec: ConnectionSpec,
        password: Option<String>,
        save_password: bool,
    },
    Error {
        message: String,
    },
    Activate,
}

impl From<Launch> for IpcMessage {
    fn from(l: Launch) -> Self {
        IpcMessage::Launch {
            spec: l.spec,
            password: l.password.map(|p| p.expose().to_string()),
            save_password: l.save_password,
        }
    }
}

impl IpcMessage {
    pub fn into_launch(self) -> Option<Launch> {
        match self {
            IpcMessage::Launch {
                spec,
                password,
                save_password,
            } => Some(Launch {
                spec,
                password: password.map(Secret::new),
                save_password,
            }),
            _ => None,
        }
    }
}

const ACK: &str = "ok";

/// Returns `Ok(true)` when a running instance accepted the message.
pub fn try_forward(socket: &Path, message: &IpcMessage) -> io::Result<bool> {
    let mut stream = match UnixStream::connect(socket) {
        Ok(s) => s,
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ) =>
        {
            return Ok(false);
        }
        Err(e) => return Err(e),
    };
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut line = serde_json::to_string(message).map_err(io::Error::other)?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply)?;
    Ok(reply.trim() == ACK)
}

/// Binds the socket (replacing a stale one) and calls `handler` for every message.
pub fn bind(socket: &Path) -> io::Result<UnixListener> {
    if let Some(dir) = socket.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let _ = std::fs::remove_file(socket);
    let listener = UnixListener::bind(socket)?;
    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

pub async fn serve<F>(listener: UnixListener, handler: F)
where
    F: Fn(IpcMessage) + Send + Sync + 'static,
{
    let handler = std::sync::Arc::new(handler);
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                log::warn!("IPC accept failed: {e}");
                continue;
            }
        };
        let handler = handler.clone();
        tokio::spawn(async move {
            let (read, mut write) = stream.into_split();
            let mut line = String::new();
            if let Err(e) = tokio::io::BufReader::new(read).read_line(&mut line).await {
                log::warn!("IPC read failed: {e}");
                return;
            }
            match serde_json::from_str::<IpcMessage>(&line) {
                Ok(message) => {
                    let _ = write.write_all(format!("{ACK}\n").as_bytes()).await;
                    handler(message);
                }
                Err(e) => log::warn!("Ignoring malformed IPC message: {e}"),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    fn spec() -> ConnectionSpec {
        ConnectionSpec {
            name: None,
            host: "127.0.0.1".into(),
            port: 3306,
            socket: None,
            user: "wp".into(),
            database: Some("wordpress".into()),
        }
    }

    #[test]
    fn forwarding_without_running_instance_reports_not_delivered() {
        let dir = tempfile::tempdir().unwrap();
        let delivered = try_forward(&dir.path().join("none.sock"), &IpcMessage::Activate).unwrap();
        assert!(!delivered);
    }

    #[tokio::test]
    async fn running_instance_receives_forwarded_launch_with_password() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("db.sock");
        let (tx, mut rx) = mpsc::unbounded_channel();
        let listener = bind(&socket).unwrap();
        tokio::spawn(serve(listener, move |m| {
            let _ = tx.send(m);
        }));

        let launch = Launch {
            spec: spec(),
            password: Some(Secret::new("pw")),
            save_password: true,
        };
        let message = IpcMessage::from(launch.clone());
        let delivered = tokio::task::spawn_blocking(move || try_forward(&socket, &message))
            .await
            .unwrap()
            .unwrap();

        assert!(delivered);
        assert_eq!(rx.recv().await.unwrap().into_launch(), Some(launch));
    }

    #[tokio::test]
    async fn socket_is_private_to_the_user() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("db.sock");
        let _listener = bind(&socket).unwrap();
        let mode = std::fs::metadata(&socket).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
