# 0002. IPC only through `api.ts` files, one per module

## Status

Accepted (retroactive, introduced in ed08cd1, 2026-10-15), amended 2026-10-20.

The clause on a module without its own folder (the WordPress lens) has lapsed: the lens was deleted in U12
([0020](0020-everything-is-rows.md), [0022](0022-presets-and-projects.md)), so every Tauri import is in
`src/api/index.ts` or a module's own `api.ts`.

## Context

UI tests need to replace the backend. `invoke` and `listen` calls scattered over components make that
impossible without mocking Tauri itself, and they hide the contract between page and backend. Optional
modules (Stage, WordPress) also need to be removable as a folder, which a single shared API file prevents.

## Decision

Only API files import `@tauri-apps/api` or a `@tauri-apps/plugin-*` package: the core's
`src/api/index.ts`, and one `src/<module>/api.ts` per module that has its own folder, with a matching mock
for tests. Everything else imports the `api` object. A boundary test fails on any other file that imports
Tauri. This per-module form replaces the original rule of a single `src/api/index.ts`; a module without its
own folder (the WordPress lens) keeps its wrappers in the core file.

## Consequences

- Tests mock one module (`vi.mock` of the API file) per backend; the contract is visible in one place.
- Every command needs a thin hand-written wrapper.
- Until a module has its own `api.ts`, its wrappers keep it coupled to the core file.
