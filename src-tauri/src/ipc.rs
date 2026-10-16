//! Single-instance handoff: a second `databee` process sends its launch request over a
//! Unix socket (mode 0600, same user only) to the running app and exits, so a forwarded
//! password never appears in the running app's argv. The process that becomes the app
//! keeps its own argv, including a password given in the URL.

use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

use crate::cli::Launch;
use crate::connection::{ConnectionSpec, Secret};

#[derive(Serialize, Deserialize, Clone, PartialEq)]
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
    /// Runs JavaScript in the window (used by scripts/dev to drive the UI).
    #[cfg(debug_assertions)]
    Eval {
        js: String,
    },
}

impl std::fmt::Debug for IpcMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpcMessage::Launch {
                spec,
                password,
                save_password,
            } => f
                .debug_struct("Launch")
                .field("spec", spec)
                .field("password", &password.as_ref().map(|_| "<redacted>"))
                .field("save_password", save_password)
                .finish(),
            IpcMessage::Error { message } => {
                f.debug_struct("Error").field("message", message).finish()
            }
            IpcMessage::Activate => f.write_str("Activate"),
            #[cfg(debug_assertions)]
            IpcMessage::Eval { js } => f.debug_struct("Eval").field("js", js).finish(),
        }
    }
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
const MAX_MESSAGE_BYTES: u64 = 64 * 1024;
const CLIENT_TIMEOUT: Duration = Duration::from_secs(5);
const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forward {
    Delivered,
    /// Nothing listens on the socket; this process may become the app.
    NoInstance,
    /// An instance listens but did not acknowledge the message.
    Rejected,
}

fn no_listener(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
    )
}

/// A timeout or other socket error is an `Err`: an instance may be alive but hung.
pub fn try_forward(socket: &Path, message: &IpcMessage) -> io::Result<Forward> {
    let mut stream = match UnixStream::connect(socket) {
        Ok(s) => s,
        Err(e) if no_listener(&e) => return Ok(Forward::NoInstance),
        Err(e) => return Err(e),
    };
    stream.set_read_timeout(Some(CLIENT_TIMEOUT))?;
    let mut line = serde_json::to_string(message).map_err(io::Error::other)?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply)?;
    Ok(if reply.trim() == ACK {
        Forward::Delivered
    } else {
        Forward::Rejected
    })
}

/// Binds the socket, replacing a stale file but never a live listener. Blocking std
/// listener, so it can be created outside the async runtime; `serve` converts it.
pub fn bind(socket: &Path) -> io::Result<UnixListener> {
    match UnixStream::connect(socket) {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                "another DataBee instance is listening on the socket",
            ));
        }
        Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => std::fs::remove_file(socket)?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let listener = UnixListener::bind(socket)?;
    listener.set_nonblocking(true)?;
    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Calls `handler` for every message received on the listener.
pub async fn serve<F>(listener: UnixListener, handler: F)
where
    F: Fn(IpcMessage) + Send + Sync + 'static,
{
    let listener = match tokio::net::UnixListener::from_std(listener) {
        Ok(l) => l,
        Err(e) => {
            log::error!("Cannot serve IPC socket: {e}");
            return;
        }
    };
    let handler = std::sync::Arc::new(handler);
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                log::warn!("IPC accept failed: {e}");
                tokio::time::sleep(ACCEPT_BACKOFF).await;
                continue;
            }
        };
        let handler = handler.clone();
        tokio::spawn(async move {
            match tokio::time::timeout(CLIENT_TIMEOUT, receive(stream)).await {
                Ok(Some(message)) => handler(message),
                Ok(None) => {}
                Err(_) => log::warn!("IPC client timed out"),
            }
        });
    }
}

/// Reads one message and acknowledges it; `None` when the client is refused.
async fn receive(stream: tokio::net::UnixStream) -> Option<IpcMessage> {
    if !same_user(&stream) {
        log::warn!("Refusing IPC connection from another user");
        return None;
    }
    let (read, mut write) = stream.into_split();
    let mut line = String::new();
    if let Err(e) = tokio::io::BufReader::new(read.take(MAX_MESSAGE_BYTES))
        .read_line(&mut line)
        .await
    {
        log::warn!("IPC read failed: {e}");
        return None;
    }
    if !line.ends_with('\n') {
        log::warn!("Ignoring incomplete or oversized IPC message");
        return None;
    }
    let message = match serde_json::from_str::<IpcMessage>(&line) {
        Ok(message) => message,
        Err(e) => {
            log::warn!("Ignoring malformed IPC message: {e}");
            return None;
        }
    };
    if let Err(e) = write.write_all(format!("{ACK}\n").as_bytes()).await {
        log::debug!("IPC acknowledgement failed: {e}");
    }
    Some(message)
}

fn same_user(stream: &tokio::net::UnixStream) -> bool {
    // SAFETY: getuid has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    stream.peer_cred().is_ok_and(|cred| cred.uid() == uid)
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

    fn serve_into_channel(socket: &Path) -> mpsc::UnboundedReceiver<IpcMessage> {
        let (tx, rx) = mpsc::unbounded_channel();
        let listener = bind(socket).unwrap();
        tokio::spawn(serve(listener, move |m| {
            let _ = tx.send(m);
        }));
        rx
    }

    async fn forward(socket: &Path, message: IpcMessage) -> Forward {
        let socket = socket.to_path_buf();
        tokio::task::spawn_blocking(move || try_forward(&socket, &message))
            .await
            .unwrap()
            .unwrap()
    }

    /// Sends raw bytes and returns whatever the server answers before closing.
    async fn raw_exchange(socket: &Path, bytes: &[u8]) -> String {
        let mut stream = tokio::net::UnixStream::connect(socket).await.unwrap();
        let _ = stream.write_all(bytes).await;
        let mut reply = String::new();
        let _ = stream.read_to_string(&mut reply).await;
        reply
    }

    #[test]
    fn forwarding_without_running_instance_reports_no_instance() {
        let dir = tempfile::tempdir().unwrap();
        let forwarded = try_forward(&dir.path().join("none.sock"), &IpcMessage::Activate).unwrap();
        assert_eq!(forwarded, Forward::NoInstance);
    }

    #[test]
    fn stale_socket_file_means_no_instance_and_is_replaced_by_bind() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("db.sock");
        drop(UnixListener::bind(&socket).unwrap());
        assert!(socket.exists());

        assert_eq!(
            try_forward(&socket, &IpcMessage::Activate).unwrap(),
            Forward::NoInstance
        );
        assert!(bind(&socket).is_ok());
    }

    #[test]
    fn bind_refuses_while_a_listener_is_alive() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("db.sock");
        let _alive = bind(&socket).unwrap();
        let err = bind(&socket).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AddrInUse);
    }

    #[test]
    fn instance_that_closes_without_ack_rejects_the_message() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("db.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(stream).read_line(&mut line).unwrap();
        });

        let forwarded = try_forward(&socket, &IpcMessage::Activate).unwrap();
        server.join().unwrap();
        assert_eq!(forwarded, Forward::Rejected);
    }

    #[tokio::test]
    async fn malformed_line_gets_no_ack_and_reaches_no_handler() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("db.sock");
        let mut rx = serve_into_channel(&socket);

        assert_eq!(raw_exchange(&socket, b"{\"type\": nope\n").await, "");

        assert_eq!(
            forward(&socket, IpcMessage::Activate).await,
            Forward::Delivered
        );
        assert_eq!(rx.recv().await.unwrap(), IpcMessage::Activate);
    }

    #[tokio::test]
    async fn line_over_64_kib_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("db.sock");
        let mut rx = serve_into_channel(&socket);

        let mut huge = br#"{"type":"error","message":""#.to_vec();
        huge.extend(std::iter::repeat_n(b'a', 70 * 1024));
        huge.extend_from_slice(b"\"}\n");
        assert_eq!(raw_exchange(&socket, &huge).await, "");

        assert_eq!(
            forward(&socket, IpcMessage::Activate).await,
            Forward::Delivered
        );
        assert_eq!(rx.recv().await.unwrap(), IpcMessage::Activate);
    }

    #[tokio::test]
    async fn error_and_activate_messages_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("db.sock");
        let mut rx = serve_into_channel(&socket);

        for message in [
            IpcMessage::Error {
                message: "boom".into(),
            },
            IpcMessage::Activate,
        ] {
            assert_eq!(forward(&socket, message.clone()).await, Forward::Delivered);
            assert_eq!(rx.recv().await.unwrap(), message);
        }
    }

    #[tokio::test]
    async fn two_clients_at_once_are_both_served() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("db.sock");
        let mut rx = serve_into_channel(&socket);

        let first = IpcMessage::Error {
            message: "first".into(),
        };
        let second = IpcMessage::Error {
            message: "second".into(),
        };
        let (a, b) = tokio::join!(
            forward(&socket, first.clone()),
            forward(&socket, second.clone())
        );
        assert_eq!((a, b), (Forward::Delivered, Forward::Delivered));

        let mut received = vec![rx.recv().await.unwrap(), rx.recv().await.unwrap()];
        received.sort_by_key(|m| format!("{m:?}"));
        assert_eq!(received, vec![first, second]);
    }

    #[test]
    fn debug_output_redacts_the_launch_password() {
        let message = IpcMessage::Launch {
            spec: spec(),
            password: Some("pw-secret".into()),
            save_password: false,
        };
        assert!(!format!("{message:?}").contains("pw-secret"));
    }

    #[tokio::test]
    async fn running_instance_receives_forwarded_launch_with_password() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("db.sock");
        let mut rx = serve_into_channel(&socket);

        let launch = Launch {
            spec: spec(),
            password: Some(Secret::new("pw")),
            save_password: true,
        };
        let message = IpcMessage::from(launch.clone());
        let forwarded = forward(&socket, message).await;

        assert_eq!(forwarded, Forward::Delivered);
        assert_eq!(rx.recv().await.unwrap().into_launch(), Some(launch));
    }

    #[test]
    fn socket_is_private_to_the_user() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("db.sock");
        let _listener = bind(&socket).unwrap();
        let mode = std::fs::metadata(&socket).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
