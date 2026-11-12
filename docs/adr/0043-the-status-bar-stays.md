# 0043. The status bar stays

## Status

Accepted (2026-11-12), owner's decision. Records DataBee's pick of the family catalog's Choice status-bar
(option: a status bar; reference: Taskboard ADR 0060).

## Context

The family catalog asks each app to choose between a status bar for app facts and none. DataBee has had one
from the start (`components/StatusBar.tsx`): the hovered value's provenance and address, pending changes, live
mode and the sidebar toggle. It was never recorded.

## Decision

Keep the status bar. Rejected: no status bar (the provenance of the hovered value and the pending changes
would need another home in every view).

## Consequences

- None in code; the choice is recorded so the catalog's Apps line can name it.
