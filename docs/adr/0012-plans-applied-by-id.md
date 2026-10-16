# 0012. Planned changes are stored server-side and applied by id

## Status

Accepted

## Context

Every write follows plan, review, apply, undo, and the rule is that the webview never assembles a write. But
the page held the planned SQL and sent it back as text, so anything in the webview (a bug, or injected
content rendered from the database) could execute arbitrary statements past the destructive-statement check.

## Decision

The backend keeps each plan in the session under an id. The page receives a description and the statements
for display only, and applies or undoes by id. Re-validating the SQL the page sends was rejected, because it
cannot prove the text equals what was planned. Signing plans was rejected as more machinery for the same
guarantee.

## Consequences

- The webview can only trigger writes the backend planned.
- Undo history lives with the session and dies on disconnect; reconnecting loses pending plans.
- Plans hold memory until applied or discarded; the applied history is capped.
