# 0017. Keep `panic = "unwind"` so a panic in one command does not take the app down

## Status

Accepted

## Context

The release profile used `panic = "abort"`, the Tauri template default for smaller binaries. DataBee holds
open editor connections, possibly with a user transaction in progress and a query running. With `abort`, a
panic in any command handler or spawned task killed the whole process and took that state with it. Tauri and
tokio already isolate unwinding panics per command and per task, and binary size matters little for a local
desktop tool.

## Decision

Use the default unwinding strategy. Panics are bugs, but survivable ones: the failing command or task ends,
everything else keeps running. Runtime-reachable `unwrap`/`expect` calls with a plausible failure are
replaced by an `AppError` or a fallback; provably infallible ones (constant data, checked invariants) stay.
`abort` was rejected because one bug would cost the user every open session; catching panics by hand
(`catch_unwind` around each command) was rejected because the runtime already does it.

## Consequences

- A failing command returns an error to the page, and the app keeps its sessions and transactions.
- The binary is slightly larger (unwind tables and landing pads).
- A panic while a `std::sync::Mutex` or `RwLock` guard is held poisons the lock, so every lock user must
  handle poisoning (or use a lock without it, such as tokio's or `parking_lot`'s) instead of calling
  `unwrap()`.
- State a panicking call left half-updated stays visible to later calls; guarded sections are kept to single
  updates so this cannot leave data inconsistent.
