# 0014. TypeScript IPC types are generated from Rust with ts-rs

## Status

Accepted

## Context

The page and the backend share dozens of types over IPC. The hand-written TypeScript mirror drifted: string
unions the backend did not enforce, flags hidden in labels, casts at use sites, MySQL semantics guessed in the
UI. Modules also need their own type files so they can be removed as a folder.

## Decision

Rust is the source of truth. `ts-rs` derives generate the TypeScript types, the output is committed, and lint
fails when regenerating changes it. `specta`/`tauri-specta` was rejected: it also generates command bindings,
but ties the build to its command macro and is less stable across Tauri releases. A hand-written mirror with
tests was rejected because it still drifts and doubles the work; JSON Schema because it adds a format nobody
reads.

## Consequences

- A shape change in Rust shows up as a TypeScript compile error.
- The build gains a codegen step, and generated files appear in review diffs.
- 64-bit integers need an explicit mapping decision (here: `number` everywhere).
- Command signatures themselves are still hand-written in the `api` object.
