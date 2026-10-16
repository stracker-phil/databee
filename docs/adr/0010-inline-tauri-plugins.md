# 0010. Optional features are inline Tauri plugins with one install line

## Status

Accepted

## Context

Stage (and later WordPress) registered its commands in the core `generate_handler!`, read core settings and
was restarted from core commands. Removing it touched many files, and a forgotten registration only fails at
runtime. Cargo features would keep the code interleaved and multiply the builds; separate crates are heavy for
one app.

## Decision

Each optional feature is an inline `tauri::plugin::Builder` plugin exposing `init(config_dir)`, with its own
commands, managed state, settings file and capability. The core manages `AppState` before installing plugins,
so plugins may read it; the core never reads a plugin's state.

## Consequences

- Removing a feature is deleting its folder plus one line in `lib.rs` and one in `build.rs`.
- Plugin commands are namespaced (`plugin:<name>|cmd`).
- The command list in `build.rs` must be kept in sync with the plugin; a test guards it.
- Each feature carries a little more boilerplate.
