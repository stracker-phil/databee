//! The Stage hook's transport (docs/stage.md §2.2): `GET|POST
//! http://127.0.0.1:<port>/v1/stage/<verb>` with `Authorization: Bearer <token>`. One request per
//! connection; only local tools may use it (no Origin header, a loopback Host).

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Map, Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::hooks::Hub;
use super::protocol::{Params, StageError, StageResult};

pub const MAX_BODY_BYTES: usize = 16 * 1024;
const MAX_HEAD_BYTES: usize = 8 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// A request line and its headers (names lowercased).
#[derive(Debug, Clone, PartialEq)]
pub struct Head {
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: Vec<(String, String)>,
}

impl Head {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    fn content_length(&self) -> StageResult<usize> {
        if self
            .header("transfer-encoding")
            .is_some_and(|v| !v.eq_ignore_ascii_case("identity"))
        {
            return Err(StageError::new(411, "Send the body with a Content-Length."));
        }
        match self.header("content-length") {
            None => Ok(0),
            Some(v) => v
                .trim()
                .parse()
                .map_err(|_| StageError::bad("Content-Length isn't a number.")),
        }
    }
}

/// The head ends at the first blank line; None while it is incomplete.
pub fn head_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| i + 4)
}

pub fn parse_head(raw: &[u8]) -> StageResult<Head> {
    let text = std::str::from_utf8(raw).map_err(|_| StageError::bad("The request isn't UTF-8."))?;
    let mut lines = text.split("\r\n");
    let mut line = lines.next().unwrap_or_default().split(' ');
    let (Some(method), Some(target)) = (line.next(), line.next()) else {
        return Err(StageError::bad("Malformed request line."));
    };
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let headers = lines
        .take_while(|l| !l.is_empty())
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    Ok(Head {
        method: method.into(),
        path: path.into(),
        query: query.into(),
        headers,
    })
}

/// Timing-safe, so the token can't be guessed byte by byte.
fn token_matches(given: Option<&str>, expected: &str) -> bool {
    let Some(given) = given else { return false };
    if expected.is_empty() || given.len() != expected.len() {
        return false;
    }
    given
        .bytes()
        .zip(expected.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

/// The verb, once the request passed the checks that need no body.
pub fn check(head: &Head, token: &str) -> StageResult<String> {
    // Only local tools: a browser page (Origin) or a foreign Host (DNS rebinding) is refused.
    let host = head.header("host").unwrap_or_default();
    let host = host.rsplit_once(':').map_or(host, |(h, port)| {
        if port.bytes().all(|b| b.is_ascii_digit()) {
            h
        } else {
            host
        }
    });
    if head.header("origin").is_some() || (host != "127.0.0.1" && host != "localhost") {
        return Err(StageError::new(403, "Only local tools may use this API."));
    }
    if head.method != "GET" && head.method != "POST" {
        return Err(StageError::new(405, "Use GET or POST."));
    }
    let verb = head
        .path
        .strip_prefix("/v1/stage/")
        .map(|v| v.trim_end_matches('/'))
        .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_lowercase()))
        .ok_or_else(|| {
            StageError::new(404, "Use /v1/stage/<verb>: describe, window, act or state.")
        })?;
    let bearer = head.header("authorization").and_then(|v| {
        let (scheme, value) = v.split_once(' ')?;
        scheme.eq_ignore_ascii_case("bearer").then(|| value.trim())
    });
    if !token_matches(bearer, token) {
        return Err(StageError::new(401, "Wrong or missing token."));
    }
    Ok(verb.to_string())
}

/// Query string, then a JSON object of primitives (nested values are ignored) or a form body.
pub fn params(head: &Head, body: &[u8]) -> StageResult<Params> {
    let mut params: Params = url::form_urlencoded::parse(head.query.as_bytes())
        .into_owned()
        .collect();
    let body = std::str::from_utf8(body)
        .map_err(|_| StageError::bad("The body isn't UTF-8."))?
        .trim();
    if head.method != "POST" || body.is_empty() {
        return Ok(params);
    }
    let json = head
        .header("content-type")
        .is_some_and(|t| t.contains("application/json"))
        || body.starts_with('{');
    if json {
        let parsed: Value = serde_json::from_str(body)
            .map_err(|_| StageError::bad("The body isn't valid JSON."))?;
        let Value::Object(object) = parsed else {
            return Err(StageError::bad("The body must be a JSON object."));
        };
        for (key, value) in object {
            let text = match value {
                Value::String(s) => s,
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => continue,
            };
            params.insert(key, text);
        }
    } else {
        params.extend(url::form_urlencoded::parse(body.as_bytes()).into_owned());
    }
    Ok(params)
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        411 => "Length Required",
        413 => "Payload Too Large",
        _ => "Internal Server Error",
    }
}

pub fn response(result: StageResult<Map<String, Value>>) -> (u16, Vec<u8>) {
    let (status, body) = match result {
        Ok(mut out) => {
            let mut body = Map::new();
            body.insert("ok".into(), json!(true));
            body.append(&mut out);
            (200, Value::Object(body))
        }
        Err(e) => (e.status, json!({ "ok": false, "error": e.message })),
    };
    let body = format!("{body}\n");
    let raw = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: application/json; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        reason(status),
        body.len()
    );
    (status, raw.into_bytes())
}

async fn read_request(stream: &mut TcpStream, token: &str) -> StageResult<(String, Head, Vec<u8>)> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 4096];
    let end = loop {
        if let Some(end) = head_end(&buffer) {
            break end;
        }
        if buffer.len() > MAX_HEAD_BYTES {
            return Err(StageError::new(413, "The request headers are too large."));
        }
        let n = stream
            .read(&mut chunk)
            .await
            .map_err(|e| StageError::bad(e.to_string()))?;
        if n == 0 {
            return Err(StageError::bad("The request ended early."));
        }
        buffer.extend_from_slice(&chunk[..n]);
    };
    let head = parse_head(&buffer[..end])?;
    let verb = check(&head, token)?;
    let length = head.content_length()?;
    if length > MAX_BODY_BYTES {
        return Err(StageError::new(413, "The request body is too large."));
    }
    let mut body = buffer[end..].to_vec();
    while body.len() < length {
        let n = stream
            .read(&mut chunk)
            .await
            .map_err(|e| StageError::bad(e.to_string()))?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);
    Ok((verb, head, body))
}

async fn handle(mut stream: TcpStream, token: Arc<str>, hub: Arc<Hub>) {
    let result = match tokio::time::timeout(READ_TIMEOUT, read_request(&mut stream, &token)).await {
        Err(_) => Err(StageError::bad("The request took too long.")),
        Ok(Err(e)) => Err(e),
        Ok(Ok((verb, head, body))) => match params(&head, &body) {
            Ok(params) => hub.run(&verb, &params).await,
            Err(e) => Err(e),
        },
    };
    if let Err(e) = &result
        && e.status >= 500
    {
        log::warn!("Stage request failed: {}", e.message);
    }
    let (_, raw) = response(result);
    let _ = stream.write_all(&raw).await;
    let _ = stream.shutdown().await;
}

pub async fn serve(listener: TcpListener, token: Arc<str>, hub: Arc<Hub>) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(handle(stream, token.clone(), hub.clone()));
            }
            Err(e) => log::warn!("Stage API accept failed: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(raw: &str) -> Head {
        parse_head(raw.as_bytes()).unwrap()
    }

    const OK: &str = "GET /v1/stage/describe?x=1 HTTP/1.1\r\nHost: 127.0.0.1:27184\r\nAuthorization: Bearer secret\r\n\r\n";

    #[test]
    fn finds_the_end_of_the_head() {
        assert_eq!(head_end(b"GET / HTTP/1.1\r\nHost: x\r\n\r\nbody"), Some(27));
        assert_eq!(head_end(b"GET / HTTP/1.1\r\nHost: x\r\n"), None);
    }

    #[test]
    fn parses_the_request_line_and_headers() {
        let h = head(OK);
        assert_eq!(
            (h.method.as_str(), h.path.as_str(), h.query.as_str()),
            ("GET", "/v1/stage/describe", "x=1")
        );
        assert_eq!(h.header("authorization"), Some("Bearer secret"));
        assert_eq!(check(&h, "secret").unwrap(), "describe");
    }

    #[test]
    fn refuses_browsers_foreign_hosts_methods_paths_and_tokens() {
        let status = |raw: String| check(&head(&raw), "secret").unwrap_err().status;
        assert_eq!(
            status(OK.replace(
                "Host: 127.0.0.1:27184",
                "Host: 127.0.0.1:27184\r\nOrigin: https://example.com"
            )),
            403
        );
        assert_eq!(
            status(OK.replace("127.0.0.1:27184", "evil.test:27184")),
            403
        );
        assert_eq!(status(OK.replace("GET", "PUT")), 405);
        assert_eq!(status(OK.replace("/v1/stage/describe", "/v1/state")), 404);
        assert_eq!(status(OK.replace("Bearer secret", "Bearer secreT")), 401);
        assert_eq!(
            status(OK.replace("Authorization: Bearer secret\r\n", "")),
            401
        );
        assert_eq!(
            check(&head(&OK.replace("127.0.0.1:27184", "localhost")), "secret").unwrap(),
            "describe"
        );
        assert_eq!(
            check(&head(&OK.replace("Bearer", "bearer")), "secret").unwrap(),
            "describe"
        );
        // An empty configured token never matches.
        assert_eq!(
            check(&head(&OK.replace("Bearer secret", "Bearer ")), "")
                .unwrap_err()
                .status,
            401
        );
    }

    #[test]
    fn reads_flat_params_from_query_json_and_form() {
        let post = head(
            "POST /v1/stage/act?kind=click HTTP/1.1\r\nContent-Type: application/json\r\n\r\n",
        );
        let p = params(
            &post,
            br#"{"anchor":"toolbar:settings","x":10,"on":true,"nested":{"a":1}}"#,
        )
        .unwrap();
        assert_eq!(p["kind"], "click");
        assert_eq!(p["anchor"], "toolbar:settings");
        assert_eq!(p["x"], "10");
        assert_eq!(p["on"], "true");
        assert!(!p.contains_key("nested"));

        let form = head(
            "POST /v1/stage/act HTTP/1.1\r\nContent-Type: application/x-www-form-urlencoded\r\n\r\n",
        );
        let p = params(&form, b"kind=type&text=Hello+world").unwrap();
        assert_eq!(p["text"], "Hello world");

        assert_eq!(params(&post, b"{nope").unwrap_err().status, 400);
        assert_eq!(params(&post, b"[1]").unwrap_err().status, 400);
    }

    #[test]
    fn bodies_need_a_length_within_the_limit() {
        let chunked = head("POST /v1/stage/act HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n");
        assert_eq!(chunked.content_length().unwrap_err().status, 411);
        let sized = head("POST /v1/stage/act HTTP/1.1\r\nContent-Length: 12\r\n\r\n");
        assert_eq!(sized.content_length().unwrap(), 12);
    }

    #[test]
    fn responses_are_json_with_ok() {
        let mut out = Map::new();
        out.insert("state".into(), json!({}));
        let (status, raw) = response(Ok(out));
        let raw = String::from_utf8(raw).unwrap();
        assert_eq!(status, 200);
        assert!(raw.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(raw.ends_with("\r\n\r\n{\"ok\":true,\"state\":{}}\n"));
        let (status, raw) = response(Err(StageError::new(409, "busy")));
        assert_eq!(status, 409);
        assert!(
            String::from_utf8(raw)
                .unwrap()
                .ends_with("{\"ok\":false,\"error\":\"busy\"}\n")
        );
    }

    #[tokio::test]
    async fn serves_requests_over_tcp() {
        use crate::stage::hooks::tests::{FakePage, hub_with};
        let (hub, _) = hub_with(FakePage::default());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(serve(listener, Arc::from("secret"), hub));

        let send = |raw: String| async move {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            stream.write_all(raw.as_bytes()).await.unwrap();
            let mut reply = String::new();
            stream.read_to_string(&mut reply).await.unwrap();
            reply
        };
        let body = r#"{"kind":"command","command":"settings.open"}"#;
        let reply = send(format!(
            "POST /v1/stage/act HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer secret\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ))
        .await;
        assert!(
            reply.ends_with("{\"ok\":true,\"command\":\"settings.open\"}\n"),
            "{reply}"
        );

        let reply = send(format!(
            "POST /v1/stage/act HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer secret\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY_BYTES + 1
        ))
        .await;
        assert!(reply.starts_with("HTTP/1.1 413"), "{reply}");
        server.abort();
    }
}
