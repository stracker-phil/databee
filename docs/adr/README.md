# Architecture decision records

Each record (`NNNN-<slug>.md`, Nygard format: Status, Context, Decision, Consequences) explains why, in at most a page;
the docs pages explain how. An accepted decision is never edited: a new record supersedes it.

| No. | Title | Status |
|---|---|---|
| [0010](0010-inline-tauri-plugins.md) | Optional features are inline Tauri plugins with one install line | Accepted |
| [0011](0011-database-trait-object.md) | Driver seam is a `Database` trait object with a `Dialect` value | Accepted |
| [0012](0012-plans-applied-by-id.md) | Planned changes are stored server-side and applied by id | Accepted |
| [0013](0013-session-extensions.md) | Per-session module data lives in `Session.ext` | Accepted |
| [0014](0014-generated-ipc-types.md) | TypeScript IPC types are generated from Rust with ts-rs | Accepted |
| [0016](0016-module-settings-files.md) | Module settings live in the module's own JSON file | Accepted |
| [0017](0017-panic-unwind.md) | Keep `panic = "unwind"` so a panic in one command does not take the app down | Accepted |
| [0018](0018-escaping-follows-sql-mode.md) | Value escaping follows the server's `sql_mode` | Accepted |
| [0019](0019-test-doubles-behind-test-util.md) | Test doubles ship behind a `test-util` feature | Accepted |
