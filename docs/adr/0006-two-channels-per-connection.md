# 0006. Two channels per connection

## Status

Accepted (retroactive, introduced in 6dfacd4, 2026-10-15)

## Context

User SQL changes session state (`USE`, variables, `SET`, temporary tables, open transactions) and can run
for a long time. Metadata queries for the sidebar, completion and the WordPress lens must neither disturb
that state nor wait behind a slow query.

## Decision

Every session opens a dedicated editor connection for user statements and a small metadata pool for
everything else. Pool queries use qualified `` `db`.`table` `` names because they do not share the editor's
`USE`. The editor's thread id is kept so a running statement can be cancelled.

## Consequences

- Session semantics match a terminal client, and cancel works by thread id.
- Each session costs several server connections.
- The editor connection needs its own reconnect logic, and state set on it does not apply to the pool.
