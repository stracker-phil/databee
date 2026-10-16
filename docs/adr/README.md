# Architecture decision records

Each record (`NNNN-<slug>.md`, Nygard format: Status, Context, Decision, Consequences) explains why, in at most a page;
the docs pages explain how. An accepted decision is never edited: a new record supersedes it.

| No. | Title | Status |
|---|---|---|
| [0011](0011-database-trait-object.md) | Driver seam is a `Database` trait object with a `Dialect` value | Accepted |
| [0017](0017-panic-unwind.md) | Keep `panic = "unwind"` so a panic in one command does not take the app down | Accepted |
| [0018](0018-escaping-follows-sql-mode.md) | Value escaping follows the server's `sql_mode` | Accepted |
| [0019](0019-test-doubles-behind-test-util.md) | Test doubles ship behind a `test-util` feature | Accepted |
