---
description: Rust test writing conventions
paths:
  - "src-tauri/tests/**"
  - "dev/fixture.sql"
---

Hard rules about how to write Rust tests. Violations waste time and money.

**Why:** Rust tests written outside the `write-rust-unit-test` agent drift from the suite's conventions
(pure cores tested in the file's `mod tests`, exact SQL and JSON assertions, table-driven cases, `#[ignore]`
integration tests against both fixture servers, writes only into `wp_write`).

**How to apply:** Whenever Rust tests or Docker fixture data need to be written or updated, including a
`#[cfg(test)] mod tests` inside a source file, spawn the `write-rust-unit-test` agent with full context (the
code under test and the behaviors to cover). Do not write test code inline. That agent never runs what it
writes: verify its output with the `ci` agent.
