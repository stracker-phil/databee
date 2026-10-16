# DataBee

Minimalistic SQL client for senior WordPress developers working on local sites: MySQL/MariaDB
(often DDEV/Docker, dummy data), SQLite later. Not for site admins or shop owners; the WordPress
layer follows `.claude/rules/wp-layer.md`. See README.md for features and CLI, docs/roadmap.md for ideas.

## Stack
- Rust backend in `src-tauri/` (rustup stable; Homebrew rustup is keg-only: `/opt/homebrew/opt/rustup/bin`)
- Tauri 2 + React 19 + TypeScript in `src/`, bun as package manager
- macOS 27, Apple Silicon only

## Architecture
- Rust owns all logic; React only renders. `src/api/index.ts` is the only `invoke` caller (tests mock it).
- `src-tauri/src/`: `cli.rs` (argv), `ipc.rs` (single-instance socket), `db/` (lexer, safety, cell decoding,
  `mysql.rs` session: dedicated editor connection + metadata pool), `store/` (JSON files), `wp/` (WordPress lens:
  `context` detection, lossless `php` serialize parser, `value` trees, TOML `entities` + resolver, `options`), `stage/` (Stage presentation hook: local HTTP API +
  discovery file; the page side is `src/stage/`, anchors are `data-stage` attributes; see README).
- Frontend state: zustand store in `src/state/store.ts`; components in `src/components/`.
- Errors cross the boundary as `{ kind, message, code }` (`error.rs`); passwords use the redacting `Secret` type.

## Commands
- `bun run test`: Vitest and Rust unit tests
- `bun run test:db`: Docker fixtures (`dev/`) and integration tests (MariaDB 11.4 and MySQL 8.4)
- `bun run lint`: tsc and clippy (warnings are errors)
- `bun run app`: debug build of the .app bundle and launch it
- `scripts/install-cli.sh` / `scripts/install-app.sh`: symlink the `databee` command and the app launcher (README)
- `bun run build:app` + `scripts/dev/launch.sh`: debug build, restarted next to the terminal without stealing focus
- `scripts/dev/js.sh` / `scripts/dev/screenshot.sh`: drive and capture the running window in the background

## Workflow
Finish every step with the `ship-step` skill. Rules in `.claude/rules/`.
Never write to the user's DDEV databases; they are read-only test material.
