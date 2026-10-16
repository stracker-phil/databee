# 0005. Own Unix-socket single-instance handoff

## Status

Accepted (retroactive, introduced in 5e8032a, 2026-10-15)

## Context

`databee mysql://…` from a terminal or script must reach the running app, possibly with a password.
`tauri-plugin-single-instance` forwards the second process's argv, which would put that password into the
running app's arguments and its logs.

## Decision

DataBee runs its own handoff: a private Unix socket in the config directory (0600 in a 0700 directory, peers
with another uid dropped), one line of JSON per message, answered with an ack. A launch password travels as
a `Secret` and is kept as a one-shot entry by request id.

## Consequences

- Passwords stay out of the running app's argv and logs; the second process exits once the ack arrives.
- The code for stale sockets, start-up races (the instance lock) and permissions is ours to maintain.
- Unix sockets are macOS/Unix only, which fits a macOS-only app.
