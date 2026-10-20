# 0026. Connection health

## Status

Accepted (2026-10-20), implemented. Amends [0012](0012-plans-applied-by-id.md).

## Context

A DDEV or Docker database goes away whenever its container stops or restarts. Every call then waited on a
dead socket until its timeout, each view showed its own error, and the only way back was to disconnect and
connect again, which asked for the password and dropped the session with its pending plans, undo history
and editor state.

## Decision

Rust tracks the health of each session (`src-tauri/src/health/`): `ok`, `down` (since when, the last error,
the next retry) or `reconnecting`, sent to the page as a `session_health` event on every change.

- **Guard**: `Session::new` wraps the session's database in `health::Guarded`, a `Database` decorator.
  While a registered session is not `ok`, every call that needs the server fails at once with
  `AppError::ConnectionLost`. An error that `suggests_connection_loss` (connection or timeout) starts a
  check in the background.
- **Watch**: `health::watch` ticks once a second and pings each `ok` session every `PING_INTERVAL` (10 s)
  through `Database::ping`, a cheap bounded check; a failed ping marks it `down`.
- **Retry**: a `down` session is retried automatically after `RETRY_BACKOFF` (10, 30, then every 60 s)
  through `Database::reconnect`, with the credentials the session already holds.
- **Reconnect in place**: the same `Session` survives, so `plans`, undo history and `Session.ext` stay. The
  MySQL engine replaces only connections that are gone and restores the editor connection's database and
  changed session variables (`db/mysql/session_state.rs`).
- **Retry button**: the page's Retry (`reconnect_session` in `health/commands.rs`) runs the same retry, so
  no password prompt appears.

Reconnecting by opening a new session was rejected because it loses plans and editor state and needs the
password again. Letting each call time out on its own was rejected because every view would wait and fail
separately.

## Consequences

- A lost server shows as one state per session; views fail fast instead of hanging.
- Pending plans survive a restart of the container; only a disconnect loses them.
- Temporary tables, user variables and open transactions on the editor connection cannot be carried over.
- Every engine must implement `ping` and `reconnect`; the integration tests (`src-tauri/tests/health.rs`)
  kill connections on the Docker fixtures.
- An idle session costs one ping every 10 s.
