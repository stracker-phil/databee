# 0007. Plan, review, apply in one transaction, undo

## Status

Accepted (retroactive, introduced in 12652c5, 2026-10-15)

## Context

Users edit rows and WordPress meta directly. A silent write is hard to trust and hard to revert, and a
change made of several statements must not apply halfway.

## Decision

Every write is planned in Rust as statements plus undo statements. The plan is shown in the pending tray,
the user applies all pending plans together in one transaction, and an applied batch can be undone. Typed
SQL is not planned, but destructive statements ask for confirmation first.

## Consequences

- Writes are reviewable and revertible.
- Every kind of write needs a planner and undo logic in Rust.
- A change that cannot be undone has to say so (`canUndo`). [ADR 0012](0012-plans-applied-by-id.md) later
  moved the plans into Rust, applied by id.
