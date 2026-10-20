# DataBee technical documentation

These docs are for developers and coding agents working on the code. For what the app does and how to
use it, see the [project README](../README.md).

| Page | Covers |
|---|---|
| [architecture.md](architecture.md) | Design rules, process model, repository map, one request end to end |
| [backend.md](backend.md) | Rust core: startup, `AppState`, Tauri commands, events, CLI and single-instance IPC, connection health, addresses, persistence, watches and snapshots, secrets, errors, native menu, content security policy |
| [database.md](database.md) | MySQL driver: connection model and health, query execution, cell decoding, browsing, live check of table tabs, value counts, edits, adding an engine, statement counts |
| [presets.md](presets.md) | Presets and rules: the TOML format (including `[[icon]]`), placeholders, where user presets live, loading and errors, recipes, projects and the Project page, the Structure view, the readout and watched values, coverage, the built-in WordPress, WooCommerce and Action Scheduler presets |
| [decode.md](decode.md) | Decoding and editing stored values: PHP serialize, JSON, value trees |
| [ux.md](ux.md) | Design language: surfaces, color schemes and tokens, type, rows, inputs, overlays, value grammar, layout, readout, live mode, keys, palette |
| [frontend.md](frontend.md) | React app: boot, store, API boundary, command registry, components (table icons, readout, connection health), styling, tests |
| [stage.md](stage.md) | Optional local HTTP API for the Stage presenter app |
| [testing.md](testing.md) | Unit and integration tests, Docker fixtures, scripts that drive the app in the background |
| [adapting.md](adapting.md) | Renaming, porting to Linux or Windows, removing layers, conventions to keep |
| [adr/](adr/README.md) | Architecture decision records: why the design is the way it is |
| [roadmap.md](roadmap.md) | Ideas not built yet, version 2 first |
| [refactoring-plan.md](refactoring-plan.md) | Historical: the code review of 2026-10-16, before the UX redesign |

Suggested reading order: architecture, then the layer you are changing, then adapting.

The rules in [`.claude/rules/`](../.claude/rules/) are binding for changes:
- [`testing.md`](../.claude/rules/testing.md)
- [`commits.md`](../.claude/rules/commits.md)
- [`presets.md`](../.claude/rules/presets.md)
- [`rust-test-conventions.md`](../.claude/rules/rust-test-conventions.md)
- [`ts-test-conventions.md`](../.claude/rules/ts-test-conventions.md)
