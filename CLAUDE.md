# databee

Minimalistic SQL client for personal use: MySQL/MariaDB for WordPress/WooCommerce dev
(often DDEV/Docker), SQLite later. See README.md for features and CLI, docs/roadmap.md for ideas.

## Stack
- Rust backend in `src-tauri/` (rustup stable; Homebrew rustup is keg-only: `/opt/homebrew/opt/rustup/bin`)
- Tauri 2 + React 19 + TypeScript in `src/`, bun as package manager
- macOS 27, Apple Silicon only

## Architecture
- Rust owns all logic; React only renders. `src/api/index.ts` is the only `invoke` caller (tests mock it).
- `src-tauri/src/`: `cli.rs` (argv), `ipc.rs` (single-instance socket), `db/` (lexer, safety, cell decoding,
  `mysql.rs` session: dedicated editor connection + metadata pool), `store/` (JSON files), `wp/` (WordPress lens:
  `context` detection, lossless `php` serialize parser, `value` trees, TOML `entities` + resolver, `options`).
- Frontend state: zustand store in `src/state/store.ts`; components in `src/components/`.
- Errors cross the boundary as `{ kind, message, code }` (`error.rs`); passwords use the redacting `Secret` type.

## Commands
- `bun run test`: Vitest and Rust unit tests
- `bun run test:db`: Docker fixtures (`dev/`) and integration tests (MariaDB 11.4 and MySQL 8.4)
- `bun run lint`: tsc and clippy (warnings are errors)
- `bun run app`: debug build of the .app bundle and launch it
- `scripts/dev/screenshot.sh` and `scripts/dev/ui.sh`: capture and drive the running window for visual checks

## Workflow
Finish every step with the `ship-step` skill. Rules in `.claude/rules/`.
Never write to the user's DDEV databases; they are read-only test material.
