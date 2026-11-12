# 0047. Background loops restart after a panic

## Status

Accepted (2026-11-12), implemented. Amends 0017. Inherited from the family catalog (panics-unwind; reference:
Taskboard `src-tauri/src/lock.rs`, TimeTracker's supervised tasks).

## Context

ADR 0017 keeps `panic = "unwind"` so a panic fails one command. A panic in a long-running loop (the health
events, the health tick, the history writer) still ended that loop for the rest of the session, silently.

## Decision

- `supervise::supervise` runs those loops and starts one again a second after it panicked; a loop that returns
  is done. Socket and Stage connections need nothing extra: each runs in a task of its own.
- `[profile.release]` states `panic = "unwind"` explicitly.
- Rejected: `catch_unwind` inside each loop (every loop repeats it); restarting the app.

## Consequences

- A bug in a background loop costs a moment of that loop and an error line in the log.
