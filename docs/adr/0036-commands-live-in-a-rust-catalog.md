# 0036. Commands live in a Rust catalog

## Status

Accepted (2026-11-03), implemented. Adopted from Taskboard (its ADRs 0012 and 0043), owner's decision.
Amends [0015](0015-frontend-registries.md): command metadata no longer lives in the page's registry.

## Context

Commands were defined in the page (`src/commands/`), and the native menu repeated their ids, labels and
default keys in `MENU_ITEMS` (`menu.rs`), kept in step by a test that parsed `menu.rs`. `keybindings.json` could
not be checked against real ids. `databee command` and MCP asked the page for every command, so nothing ran
while the page was busy or the window hidden, and a typo only failed after a round trip.

## Decision

- One catalog in Rust (`bus/catalog.rs`): id, title, scope, default key, menu item (own label, or shares
  another command's item), `fallback_for`, `skip_in_text`, and where it runs (`Runs::Page` or `Runs::Rust`).
  The ids stay as they were (`tabs.close`), so saved keybindings keep working.
- The menu, the keybindings validation, the CLI (`databee commands [--json]`) and MCP read the catalog. The
  page imports it as `src/commands/catalog.gen.json`, written by `bun run types` and compared by lint.
- The page keeps key resolution, scopes and the palette, and registers handlers (`when`, `run`) for page
  commands; installing throws when a page command has no handler or a handler has no catalog entry.
- `bus/` runs commands by id: Rust commands (Settings, zoom) in Rust, page commands in the page (a
  `page_command` event for menu clicks, a `pageCommand` request for the CLI and MCP, answered through
  `remote.rs`).

Rejected:
- Taskboard's full bus with families, argument parsing and a `Host` trait: DataBee's commands take no
  arguments, and almost all need the page's state (the active tab, the selection); a catalog plus a small
  runner covers what is used.
- Moving page commands' logic into Rust: their state (selection, focus, open panes) lives in the page; Rust
  would have to mirror it.
- Keeping metadata in TypeScript and generating `menu.rs` from it: Rust would still not know the ids at run
  time, so the CLI, MCP and keybindings couldn't check them.
- The palette listing from Rust (Taskboard's ADR 0043): the page already lists every catalog command whose
  `when` passes; a Rust list would need the page's state.

## Consequences

- A new command is a catalog entry plus a page handler or a `RustAction` (docs/backend.md, Command catalog
  and bus); forgetting either fails at install or in a test, not at run time.
- `databee command settings.open` and the zoom commands work with the window hidden.
- Moving more commands to Rust (sidebar, ⌘W, quit) is a catalog flag and a handler, not a new surface.
