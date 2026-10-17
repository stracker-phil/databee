//! Single-instance handoff: a second `databee` process sends its launch request over a
//! Unix socket (mode 0600, same user only) to the running app and exits, so a forwarded
//! password never appears in the running app's argv. The process that becomes the app
//! keeps its own argv, including a password given in the URL.
//!
//! One JSON line per connection. A bare `IpcMessage` (the one-way form) is acknowledged with
//! the line `ok` before it is handled; an `IpcRequest` is answered with an `IpcReply` line
//! once it is handled.

use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tokio::net::unix::OwnedWriteHalf;

use crate::cli::Launch;
use crate::connection::{ConnectionSpec, Secret, serialize_exposed};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum IpcMessage {
    #[serde(rename_all = "camelCase")]
    Launch {
        spec: ConnectionSpec,
        #[serde(serialize_with = "serialize_exposed")]
        password: Option<Secret>,
        save_password: bool,
    },
    Error {
        message: String,
    },
    Activate,
    /// `address` is an address, or an entity shorthand (`order 4711`) in the session of
    /// `connection` (a slug; the active session when `None`).
    Open {
        address: String,
        #[serde(default)]
        connection: Option<String>,
        #[serde(default)]
        focus: bool,
    },
    /// Runs a command of the page's command registry.
    Command {
        id: String,
    },
    /// Replies with the active tab's address, or `null`.
    Current,
    /// Runs JavaScript in the window (used by scripts/dev to drive the UI).
    #[cfg(debug_assertions)]
    Eval {
        js: String,
    },
}

impl IpcMessage {
    /// The message type for logs; never its contents (passwords, scripts).
    pub fn kind(&self) -> &'static str {
        match self {
            IpcMessage::Launch { .. } => "Launch",
            IpcMessage::Error { .. } => "Error",
            IpcMessage::Activate => "Activate",
            IpcMessage::Open { .. } => "Open",
            IpcMessage::Command { .. } => "Command",
            IpcMessage::Current => "Current",
            #[cfg(debug_assertions)]
            IpcMessage::Eval { .. } => "Eval",
        }
    }
}

impl From<Launch> for IpcMessage {
    fn from(l: Launch) -> Self {
        IpcMessage::Launch {
            spec: l.spec,
            password: l.password,
            save_password: l.save_password,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct IpcRequest {
    pub id: String,
    pub message: IpcMessage,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct IpcReply {
    pub id: String,
    pub ok: bool,
    pub result: serde_json::Value,
    pub error: Option<String>,
}

impl IpcReply {
    fn new(id: String, outcome: HandlerResult) -> Self {
        match outcome {
            Ok(result) => Self {
                id,
                ok: true,
                result,
                error: None,
            },
            Err(error) => Self {
                id,
                ok: false,
                result: serde_json::Value::Null,
                error: Some(error),
            },
        }
    }

    pub fn into_result(self) -> HandlerResult {
        if self.ok {
            Ok(self.result)
        } else {
            Err(self.error.unwrap_or_else(|| "The request failed".into()))
        }
    }
}

/// What handling a message produced: a JSON result, or an error for the sender.
pub type HandlerResult = Result<serde_json::Value, String>;

#[derive(Deserialize)]
#[serde(untagged)]
enum Incoming {
    Request(IpcRequest),
    OneWay(IpcMessage),
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

/// Sends `value` as one line and reads the answer line; `None` when nothing listens.
fn send_line(socket: &Path, value: &impl Serialize) -> io::Result<Option<String>> {
    let mut stream = match UnixStream::connect(socket) {
        Ok(s) => s,
        Err(e) if no_listener(&e) => return Ok(None),
        Err(e) => return Err(e),
    };
    stream.set_read_timeout(Some(CLIENT_TIMEOUT))?;
    let mut line = serde_json::to_string(value).map_err(io::Error::other)?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    let mut answer = String::new();
    BufReader::new(stream).read_line(&mut answer)?;
    Ok(Some(answer))
}

/// A timeout or other socket error is an `Err`: an instance may be alive but hung.
pub fn try_forward(socket: &Path, message: &IpcMessage) -> io::Result<Forward> {
    Ok(match send_line(socket, message)? {
        None => Forward::NoInstance,
        Some(answer) if answer.trim() == ACK => Forward::Delivered,
        Some(_) => Forward::Rejected,
    })
}

/// Sends `message` as a request and waits for its reply; `None` when nothing listens.
pub fn request(socket: &Path, message: IpcMessage) -> io::Result<Option<IpcReply>> {
    let request = IpcRequest {
        id: uuid::Uuid::new_v4().to_string(),
        message,
    };
    let Some(answer) = send_line(socket, &request)? else {
        return Ok(None);
    };
    match serde_json::from_str::<IpcReply>(&answer) {
        Ok(reply) if reply.id == request.id => Ok(Some(reply)),
        _ => Err(io::Error::other(
            "the running DataBee instance did not answer the request; it may be older than this command",
        )),
    }
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

/// Calls `handler` for every message on the listener and writes each request's result back to its client.
pub async fn serve<F, Fut>(listener: UnixListener, handler: F)
where
    F: Fn(IpcMessage) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = HandlerResult> + Send + 'static,
{
    let listener = match tokio::net::UnixListener::from_std(listener) {
        Ok(l) => l,
        Err(e) => {
            log::error!("Cannot serve IPC socket: {e}");
            return;
        }
    };
    let handler = Arc::new(handler);
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                log::warn!("IPC accept failed: {e}");
                tokio::time::sleep(ACCEPT_BACKOFF).await;
                continue;
            }
        };
        tokio::spawn(handle_connection(stream, handler.clone()));
    }
}

async fn handle_connection<F, Fut>(stream: tokio::net::UnixStream, handler: Arc<F>)
where
    F: Fn(IpcMessage) -> Fut,
    Fut: Future<Output = HandlerResult>,
{
    let (incoming, mut write) = match tokio::time::timeout(CLIENT_TIMEOUT, receive(stream)).await {
        Ok(Some(received)) => received,
        Ok(None) => return,
        Err(_) => {
            log::warn!("IPC client timed out");
            return;
        }
    };
    match incoming {
        Incoming::OneWay(message) => {
            write_line(&mut write, ACK).await;
            let kind = message.kind();
            if let Err(e) = handler(message).await {
                log::warn!("Handling {kind} from another process failed: {e}");
            }
        }
        Incoming::Request(IpcRequest { id, message }) => {
            let reply = IpcReply::new(id, handler(message).await);
            match serde_json::to_string(&reply) {
                Ok(line) => write_line(&mut write, &line).await,
                Err(e) => log::warn!("IPC reply not serializable: {e}"),
            }
        }
    }
}

async fn write_line(write: &mut OwnedWriteHalf, line: &str) {
    if let Err(e) = write.write_all(format!("{line}\n").as_bytes()).await {
        log::debug!("IPC reply failed: {e}");
    }
}

/// Reads one message or request; `None` when the client is refused.
async fn receive(stream: tokio::net::UnixStream) -> Option<(Incoming, OwnedWriteHalf)> {
    if !same_user(&stream) {
        log::warn!("Refusing IPC connection from another user");
        return None;
    }
    let (read, write) = stream.into_split();
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
    match serde_json::from_str::<Incoming>(&line) {
        Ok(incoming) => Some((incoming, write)),
        Err(e) => {
            log::warn!("Ignoring malformed IPC message: {e}");
            None
        }
    }
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
            database: Some("site".into()),
        }
    }

    fn serve_into_channel(socket: &Path) -> mpsc::UnboundedReceiver<IpcMessage> {
        let (tx, rx) = mpsc::unbounded_channel();
        let listener = bind(socket).unwrap();
        tokio::spawn(serve(listener, move |m| {
            let _ = tx.send(m);
            async { Ok(serde_json::Value::Null) }
        }));
        rx
    }

    fn serve_answering(socket: &Path) {
        let listener = bind(socket).unwrap();
        tokio::spawn(serve(listener, |m| async move {
            match m {
                IpcMessage::Command { id } => Ok(serde_json::json!({ "ran": id })),
                other => Err(format!("cannot handle {}", other.kind())),
            }
        }));
    }

    async fn ask(socket: &Path, message: IpcMessage) -> io::Result<Option<IpcReply>> {
        let socket = socket.to_path_buf();
        tokio::task::spawn_blocking(move || request(&socket, message))
            .await
            .unwrap()
    }

    /// A listener that reads one line and answers with `answer` (nothing when empty).
    fn canned_listener(socket: &Path, answer: &'static str) -> std::thread::JoinHandle<()> {
        let listener = UnixListener::bind(socket).unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            stream.write_all(answer.as_bytes()).unwrap();
        })
    }

    #[tokio::test]
    async fn successful_request_is_answered_with_the_handler_result_under_its_id() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("db.sock");
        serve_answering(&socket);

        let reply = ask(
            &socket,
            IpcMessage::Command {
                id: "view.sidebar".into(),
            },
        )
        .await
        .unwrap()
        .unwrap();

        assert!(reply.ok);
        assert_eq!(reply.result, serde_json::json!({ "ran": "view.sidebar" }));
        assert_eq!(reply.error, None);
        assert!(!reply.id.is_empty());
    }

    #[tokio::test]
    async fn failing_request_is_answered_with_the_handler_error() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("db.sock");
        serve_answering(&socket);

        let reply = ask(&socket, IpcMessage::Current).await.unwrap().unwrap();

        assert!(!reply.ok);
        assert_eq!(reply.result, serde_json::Value::Null);
        assert_eq!(reply.error.as_deref(), Some("cannot handle Current"));
    }

    #[tokio::test]
    async fn one_way_message_is_acknowledged_with_ok_and_reaches_the_handler() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("db.sock");
        let mut rx = serve_into_channel(&socket);

        let line = format!(
            "{}\n",
            serde_json::to_string(&IpcMessage::Activate).unwrap()
        );
        assert_eq!(raw_exchange(&socket, line.as_bytes()).await, "ok\n");
        assert_eq!(rx.recv().await.unwrap(), IpcMessage::Activate);

        assert_eq!(
            forward(&socket, IpcMessage::Activate).await,
            Forward::Delivered
        );
        assert_eq!(rx.recv().await.unwrap(), IpcMessage::Activate);
    }

    #[tokio::test]
    async fn malformed_request_gets_no_answer() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("db.sock");
        serve_answering(&socket);

        let reply = raw_exchange(&socket, b"{\"id\":\"1\",\"message\":{\"type\":nope}}\n").await;

        assert_eq!(reply, "");
    }

    #[test]
    fn request_without_a_socket_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let reply = request(&dir.path().join("none.sock"), IpcMessage::Current).unwrap();
        assert_eq!(reply, None);
    }

    #[test]
    fn request_to_an_instance_that_answers_ok_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("db.sock");
        let server = canned_listener(&socket, "ok\n");

        let result = request(&socket, IpcMessage::Current);

        server.join().unwrap();
        assert!(result.is_err(), "{result:?}");
    }

    #[test]
    fn request_to_an_instance_that_closes_without_answering_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("db.sock");
        let server = canned_listener(&socket, "");

        let result = request(&socket, IpcMessage::Current);

        server.join().unwrap();
        assert!(result.is_err(), "{result:?}");
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
            password: Some(Secret::new("pw-secret")),
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
        assert_eq!(rx.recv().await.unwrap(), IpcMessage::from(launch));
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
