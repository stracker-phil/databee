# 0011. Driver seam is a `Database` trait object with a `Dialect` value

## Status

Accepted

## Context

SQLite is on the roadmap. The WordPress lens, the commands and the session code all took the concrete MySQL
session, so nothing above the driver had unit tests, and adding an engine meant touching every caller. The
engine differences callers need (identifier quoting, string escaping, system schemas) are data, not behavior.

## Decision

Callers hold `Box<dyn Database>` / `&dyn Database`, with async methods through `async_trait`. Engine-specific
text rules live in a `Dialect` value that the driver returns. An `enum Driver { MySql(..), Sqlite(..) }` was
rejected: every method becomes a match, every engine edits the enum file, and there is no room for a fake.
Generics over `D: Database` were rejected because the type parameter would spread through commands, Tauri
state and the WordPress layer.

## Consequences

- A `FakeDatabase` makes session, command and WordPress planning code unit-testable.
- A new engine is one folder plus one arm in `db::open`.
- Dynamic dispatch and boxed futures cost a little per call, negligible next to network I/O.
- The trait must stay engine-neutral, so a MySQL-only feature needs a capability method or a downcast.
