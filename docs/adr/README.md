# Architecture decision records

Each record (`NNNN-<slug>.md`, Nygard format: Status, Context, Decision, Consequences) explains why, in at most a page;
the docs pages explain how. An accepted decision is never edited: a new record supersedes it, and a dated amendment under Status
only corrects facts that changed.

| No. | Title | Status |
|---|---|---|
| [0001](0001-rust-owns-the-logic.md) | Rust owns the logic, React only renders | Accepted |
| [0002](0002-ipc-in-api-files.md) | IPC only through `api.ts` files, one per module | Accepted, amended |
| [0003](0003-errors-as-kinds.md) | Errors cross IPC as `{kind, message, code}` | Accepted |
| [0004](0004-passwords-in-secret.md) | Passwords live in `Secret` | Accepted |
| [0005](0005-own-single-instance-socket.md) | Own Unix-socket single-instance handoff | Accepted |
| [0006](0006-two-channels-per-connection.md) | Two channels per connection | Accepted |
| [0007](0007-plan-review-apply-undo.md) | Plan, review, apply in one transaction, undo | Accepted |
| [0008](0008-wp-layer-shows-stored-data.md) | The WordPress layer shows stored data, no naming heuristics | Superseded by 0022 |
| [0009](0009-script-as-one-call.md) | A script runs as one multi-statement call | Accepted |
| [0010](0010-inline-tauri-plugins.md) | Optional features are inline Tauri plugins with one install line | Accepted |
| [0011](0011-database-trait-object.md) | Driver seam is a `Database` trait object with a `Dialect` value | Accepted, amended |
| [0012](0012-plans-applied-by-id.md) | Planned changes are stored server-side and applied by id | Accepted, amended by 0026 |
| [0013](0013-session-extensions.md) | Per-session module data lives in `Session.ext` | Accepted, amended |
| [0014](0014-generated-ipc-types.md) | TypeScript IPC types are generated from Rust with ts-rs | Accepted |
| [0015](0015-frontend-registries.md) | Frontend modules contribute through registries | Accepted, amended by 0020 |
| [0016](0016-module-settings-files.md) | Module settings live in the module's own JSON file | Accepted |
| [0017](0017-panic-unwind.md) | Keep `panic = "unwind"` so a panic in one command does not take the app down | Accepted |
| [0018](0018-escaping-follows-sql-mode.md) | Value escaping follows the server's `sql_mode` | Accepted |
| [0019](0019-test-doubles-behind-test-util.md) | Test doubles ship behind a `test-util` feature | Accepted |
| [0020](0020-everything-is-rows.md) | Everything is rows: one workspace, Document and Rows views of one object | Accepted (2026-10-17), implemented, amended |
| [0021](0021-addresses.md) | Every view has an address | Accepted (2026-10-17), implemented, amended |
| [0022](0022-presets-and-projects.md) | Presets and projects instead of WordPress code | Accepted (2026-10-17), implemented |
| [0023](0023-virtual-foreign-keys.md) | References are virtual foreign keys from four sources | Accepted (2026-10-17), implemented, amended |
| [0024](0024-color-schemes.md) | Color schemes are complete token sets; no color literal outside a scheme | Accepted (2026-10-17), implemented, amended |
| [0025](0025-standalone-mcp-server.md) | A standalone, read-only MCP server (`databee mcp`) | Accepted (2026-10-17), implemented |
| [0026](0026-connection-health.md) | Connection health | Accepted (2026-10-20), implemented |
| [0027](0027-table-live-check.md) | Table live check by per-row checksums | Accepted (2026-10-20), implemented |
