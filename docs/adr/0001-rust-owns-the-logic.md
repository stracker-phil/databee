# 0001. Rust owns the logic, React only renders

## Status

Accepted (retroactive, introduced in ed08cd1, 2026-10-15)

## Context

A SQL client decides which statements are safe to run, decodes PHP-serialized values byte for byte, builds
the SQL for edits and resolves WordPress entities. Each of these must be right once and testable without a
browser. The webview also renders arbitrary database content, so it is the part most exposed to surprises.

## Decision

Every rule lives in Rust: SQL building and quoting, safety checks, cell decoding, PHP parsing and WordPress
entity resolution. The page asks for results and plans, renders them and forwards user intent; it never
assembles a statement itself. Splitting logic between the two sides was rejected because the same rule would
then exist twice, or only in the half that cannot be unit tested without a DOM.

## Consequences

- The logic is unit tested in Rust and does not depend on page code being correct.
- More IPC types and round trips, even for small things such as a `SELECT` for the Sidebar.
- A UI feature often needs a Rust change first.
