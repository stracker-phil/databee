# 0004. Passwords live in `Secret`

## Status

Accepted (retroactive, introduced in 5e8032a, 2026-10-15)

## Context

Passwords pass through the CLI, the single-instance socket, IPC, the Keychain and the log. A derived `Debug`
or a serialized settings struct is enough to leak one into a log line or a JSON file.

## Decision

Every place that holds a password uses the redacting `Secret` type: it prints as `Secret(***)`, is zeroed on
drop, has no `Serialize` impl and gives up its value only through `expose()`. Passwords are stored only in
the Keychain, never in JSON files, logs or the webview; a connect call references them by request id.

## Consequences

- A leak needs an explicit `expose()` or `serialize_exposed`, which is easy to review.
- Code that needs the plain value is more verbose.
- One-shot passwords from the CLI need their own hand-off (the launch queue) instead of travelling with the
  launch event.
