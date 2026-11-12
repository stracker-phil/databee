# DataBee technical documentation

These docs are for developers and coding agents working on the code. For what the app does and how to
use it, see the [project README](../README.md).

| Page | Covers |
|---|---|
| [architecture.md](architecture.md) | Design rules, process model, repository map, webview hardening, one request end to end |
| [backend.md](backend.md) | Rust core: startup, `AppState`, Tauri commands, events, CLI and single-instance IPC, connection health, addresses, persistence, watches and snapshots, secrets, errors, windows and quitting, native menu, command catalog and bus, content security policy |
| [database.md](database.md) | MySQL driver: connection model and health, query execution, cell decoding, browsing, live check of table tabs, value counts, edits, adding an engine, statement counts |
| [presets.md](presets.md) | Presets and rules: the TOML format (including `[[icon]]`), placeholders, where user presets live, loading and errors, recipes, projects and the Project page, the Structure view, the readout and watched values, coverage, the built-in WordPress, WooCommerce and Action Scheduler presets |
| [decode.md](decode.md) | Decoding and editing stored values: PHP serialize, JSON, value trees |
| [ux.md](ux.md) | Design language: stable controls, motion, surfaces, color schemes and tokens, type, rows, inputs, overlays, Settings, value grammar, layout, readout, live mode, keys, palette |
| [frontend.md](frontend.md) | React app: boot, store, API boundary, commands (handlers for the Rust catalog), components (Settings pages, table icons, readout, connection health), styling, tests |
| [stage.md](stage.md) | Optional local HTTP API for the Stage presenter app |
| [testing.md](testing.md) | Unit and integration tests, Docker fixtures, scripts that drive the app in the background |
| [adapting.md](adapting.md) | Renaming, porting to Linux or Windows, removing layers, conventions to keep |
| [adr/](adr/README.md) | Architecture decision records: why the design is the way it is |
| [roadmap.md](roadmap.md) | Ideas not built yet, version 2 first |

Suggested reading order: architecture, then the layer you are changing, then adapting. Pages for new layers
are added with their code.

## Sibling apps

DataBee is one app of the owner's family and shares their stack, process and look (family catalog: the
user-level `app-blueprint` skill). When this repo has no answer yet, look there (read-only):

| App | Path | Take from it |
|---|---|---|
| Taskboard | `../taskboard` | the newest decisions: scripts, ship-step, motion, tooltips, menus, palette, Settings window, CSP and navigation guard |
| Daybook | `../daybook` | the Rust command bus (`docs/commands.md`), extra windows on a native layer, menu-bar app, JSON stores with repair |
| TimeTracker | `../timetracker` | SQLite through sqlx, undo as ops, the seeds, the MCP server, `PopUpButton` |
| Launcher | `../project-launcher` | the Settings search over every row, the Shortcuts page (Reassign, Reset) |

The rules in [`.claude/rules/`](../.claude/rules/) are binding for changes:
- [`testing.md`](../.claude/rules/testing.md)
- [`commits.md`](../.claude/rules/commits.md)
- [`presets.md`](../.claude/rules/presets.md)
- [`demo-data.md`](../.claude/rules/demo-data.md)
- [`stable-controls.md`](../.claude/rules/stable-controls.md)
- [`rust-test-conventions.md`](../.claude/rules/rust-test-conventions.md)
- [`ts-test-conventions.md`](../.claude/rules/ts-test-conventions.md)
