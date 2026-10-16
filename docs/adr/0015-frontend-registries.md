# 0015. Frontend modules contribute through registries

## Status

Accepted

## Context

Stage and the WordPress lens were wired into `App`, the store, Settings and the command list directly, so
removing or swapping one meant editing several core files, and a forgotten line only showed at runtime.

## Decision

The core exposes registries: store slices (`registerSlice`), Settings pages (`registerSettingsPage`) and
command scopes (`registerScope`). Each module registers from its own install function, and `src/modules.ts`
installs the core and then each module with one call. The core's components, state, commands,
registries and helpers never import a module, which a boundary test checks; the app shell still mounts the
WordPress workspace directly. Build-time feature flags were rejected because the code stays interleaved; a plugin loader was
rejected as overkill for a single app.

## Consequences

- Removing a module is deleting its folder and its line in `src/modules.ts`.
- Registries add indirection: a module's contributions are found through `modules.ts` and the registry
  files, not by reading `App`.
- Registration order matters where a registry orders by it; pages use `order` instead.
- Parts of the WordPress lens (its store slice, commands and workspace) still live in the core.
