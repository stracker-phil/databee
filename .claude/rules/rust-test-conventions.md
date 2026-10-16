---
description: Rust tests and fixtures are written by the write-rust-unit-test agent
paths:
  - "src-tauri/tests/**"
  - "dev/fixture.sql"
---

Don't edit these files inline. Spawn the `write-rust-unit-test` agent with the code under test and the
behaviors to cover; it knows the suite's conventions. It never runs what it writes: verify its output with
the `ci` agent.
