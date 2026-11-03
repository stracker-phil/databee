# 0034. Reads stay in the store, not TanStack Query

## Status

Rejected (2026-11-03), owner's decision. Taskboard reads through TanStack Query (its ADR 0018); DataBee does
not adopt it.

## Context

Taskboard fetches its reads with TanStack Query: cached by key, refetched on invalidation. DataBee's reads
live in zustand slices (`src/state/slices/`) and in `useApiResult` (`src/lib/useApiResult.ts`) for a
component's own read. Most of them belong to one open session and change through events Rust already sends:
`session_health`, the table live check (`table_check.rs`), plan and apply results, `settings_changed`.

## Decision

Keep reads in the store and `useApiResult`. Don't add TanStack Query.

Rejected:
- TanStack Query for every read: it would add a second cache beside the store that has to agree with the
  session's lifetime, the live check and the apply flow, which already decide when data is stale.
- TanStack Query for some reads only: two ways to read for one kind of data.

## Consequences

- A new read is a store action or a `useApiResult` call; refreshing stays explicit, driven by the events
  above.
- If hand-written loading and error state starts repeating across slices, this decision can be revisited
  with a new record.
