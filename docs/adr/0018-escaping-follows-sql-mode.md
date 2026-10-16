# 0018. Value escaping follows the server's `sql_mode`

## Status

Accepted

## Context

DataBee leaves the server's `sql_mode` untouched, because a SQL client must not change session semantics
behind the user's back. But the client generates SQL literals for edits, and under `NO_BACKSLASH_ESCAPES` a
backslash-doubling escaper corrupts every written `\` (JSON, serialized PHP, LIKE patterns).

## Decision

The session reads `@@SESSION.sql_mode` at connect and reconnect and keeps `backslash_escapes` in its
`Dialect`; every generated literal goes through `Dialect::sql_string`. Forcing a fixed `sql_mode` on connect
was rejected because it changes how the user's own queries behave. Bound parameters everywhere were rejected
because the review-before-apply flow shows the exact statements as text, which needs literals.

## Consequences

- Written values are byte-exact under both modes.
- A user who changes `sql_mode` mid-session on the editor connection is not tracked until the next reconnect.
- Every new write path must use the dialect, never a local escaper.
