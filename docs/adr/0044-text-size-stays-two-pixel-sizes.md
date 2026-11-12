# 0044. Text size stays two pixel sizes

## Status

Accepted (2026-11-12), owner's decision. Deviates from the family design language (Settings → Text size
Small/Medium/Large).

## Context

The family offers text size as Small, Medium and Large with ⌘= / ⌘- / ⌘0. DataBee has two pixel fields, UI
size and Data size, and the same zoom keys, which step both.

## Decision

Keep the two pixel sizes. Rejected: Small/Medium/Large (one step for chrome and data together: a data tool
needs dense grids next to readable chrome, or the reverse on a large screen).

## Consequences

- `docs/ux.md` → Settings names the exception.
