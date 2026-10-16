# 0019. Test doubles ship behind a `test-util` feature

## Status

Accepted

## Context

Unit tests need in-memory doubles (`MemoryStore` for secrets, later a fake database) so they never touch the
Keychain or a real server. Integration tests in `src-tauri/tests/` are a separate crate and cannot see items
gated with `#[cfg(test)]`. Compiling the doubles unconditionally would put dead code into the release binary
and make it easy to wire a fake into the app by mistake.

## Decision

Doubles compile under `cfg(any(test, feature = "test-util"))`. The crate lists itself as a dev-dependency with
the `test-util` feature, so integration tests get the doubles while release builds never include them.
Always compiling them was rejected (release bloat, accidental use); a separate test-support crate was rejected
because a workspace adds overhead for a single-crate app.

## Consequences

- Doubles are available wherever tests run and absent from the app.
- Every new double must use the same `cfg` gate.
- The self dev-dependency is an unusual Cargo pattern that a reader has to know about; `Cargo.toml` points
  here.
