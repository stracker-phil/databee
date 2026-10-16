# DataBee

Minimalistic SQL client for senior WordPress developers working on local sites: MySQL/MariaDB
(often DDEV/Docker, dummy data), SQLite later. Not for site admins or shop owners; the WordPress
layer follows `.claude/rules/wp-layer.md`. See README.md for features and CLI, docs/roadmap.md for ideas.

## Docs
Technical docs live in `docs/` (index: `docs/README.md`). Read `docs/architecture.md` and the page of the
layer you change before changing it; `docs/adapting.md` lists everything macOS-, name- or WordPress-specific
(for ports, renames and removals). Update the affected docs page in the same commit when behavior,
commands, files or extension points change. Document by file and symbol, never by line number.

## Stack
- Rust backend in `src-tauri/` (rustup stable; Homebrew rustup is keg-only: `/opt/homebrew/opt/rustup/bin`)
- Tauri 2 + React 19 + TypeScript in `src/`, bun as package manager
- macOS 27, Apple Silicon only

## Architecture
- Rust owns all logic; React only renders. `src/api/index.ts` is the only `invoke` caller (tests mock it).
- `src-tauri/src/`: `cli.rs` (argv), `ipc.rs` (single-instance socket), `db/` (lexer, safety, cell decoding,
  `mysql.rs` session: dedicated editor connection + metadata pool), `store/` (JSON files), `wp/` (WordPress lens:
  `context` detection, lossless `php` serialize parser, `value` trees, TOML `entities` + resolver, `options`), `stage/` (Stage presentation hook: local HTTP API,
  registered with `stage register`; the page side is `src/stage/`, anchors are `data-stage` attributes; see docs/stage.md).
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
Finish every step with the `ship-step` skill. Rules in `.claude/rules/`. Run the `clean-up-code` skill
before every commit, also outside `ship-step`.
Never write to the user's own databases (e.g. DDEV sites); they are read-only test material. Writes in tests
and manual checks go to the Docker fixtures (`wp_write`, `databee_test`).
