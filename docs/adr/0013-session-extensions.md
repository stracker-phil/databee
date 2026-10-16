# 0013. Per-session module data lives in `Session.ext`

## Status

Accepted

## Context

Modules (the WordPress lens now, others later) need data tied to one connection, such as a detected site
context. Keeping it in global `AppState` maps keyed by session id couples the core to every module, needs
manual cleanup on disconnect, and leaks memory whenever that cleanup is missed.

## Decision

`Session` carries a type-keyed extension map; a module stores a value of its own type there and reads it back
by type. One field per module on `Session` was rejected: every module would edit the core, and a module could
not be removed by deleting its folder. Global maps keyed by session id were rejected for their lifetime bugs
and because the core would know module types.

## Consequences

- Per-session data is freed with the session, and the core never names a module type.
- Access goes through `Any` downcasts, so the compiler cannot show which modules attach what.
- Each extension type must be `Send + Sync` and handle its own interior locking.
