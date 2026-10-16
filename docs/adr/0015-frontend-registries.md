# 0015. Frontend modules contribute through registries

## Status

Accepted, amended by [0020](0020-everything-is-rows.md) (2026-10-17).

The registries stay. The WordPress/Database mode toggle is removed, so the planned mode registry
(`registerMode`) and slot registries (status bar, table row actions, palette sources) are not coming: rules
replace those contributions. WordPress becomes a preset
([0022](0022-presets-and-projects.md)), and its store slice, commands and workspace leave the core with
`src/components/wp/` instead of moving into a module.

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
