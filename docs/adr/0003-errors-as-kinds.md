# 0003. Errors cross IPC as `{kind, message, code}`

## Status

Accepted (retroactive, introduced in 5310cd0, 2026-10-15)

## Context

The page reacts to classes of errors: a missing password opens the password dialog, a lost connection
reconnects, a destructive statement asks for confirmation. Parsing English messages for that is fragile, and
the server's own error number is useful to show but not enough to branch on.

## Decision

`AppError` serializes to a stable `kind`, the display `message` and an optional `code` (the MySQL error number
or SQLSTATE), plus the Keychain `account` for login errors. The UI branches on `kind` only; `toAppError`
accepts only known kinds.

## Consequences

- UI flows such as `password_required` and `confirmation_required` are plain branches, testable with a mock.
- Adding a kind touches both sides.
- Messages may change freely without breaking the UI.
