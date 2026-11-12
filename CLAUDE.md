# DataBee

Minimalistic SQL client for senior WordPress developers working on local sites: MySQL/MariaDB
(often DDEV/Docker, dummy data), SQLite later. Not for site admins or shop owners. WordPress support is a
preset (`src-tauri/presets/`), not code; presets and projects follow `.claude/rules/presets.md`. See
README.md for features and CLI, docs/roadmap.md for ideas. Step plans live in `docs/plan/` (gitignored).

DataBee is part of the owner's family of macOS apps and inherits its architecture, process and look from the
family catalog (the user-level `app-blueprint` skill). The sibling apps:

- **Taskboard** (`../taskboard`): the newest decisions, scripts and process
- **Daybook** (`../daybook`): the command bus, extra windows, the menu bar
- **TimeTracker** (`../timetracker`): SQLite with sqlx, undo, seeds, MCP
- **Launcher** (`../project-launcher`): the Settings search and the Shortcuts page

Read them; never write to them. DataBee differs only where its job differs; each deviation has an ADR.

## Rules
- Every step ends in a commit, so it can be reverted; commit each finished part as soon as it is green.
  Never push. Tests and lint run green through the `ci` agent before every commit (`.claude/rules/commits.md`).
- Tests pin behavior, not internals: public interfaces only, no log assertions, no call counts.
- Refactor continuously: when a file grows over ~400 lines of production code or gets complex, simplify it
  and split it into focused modules in the same step, without asking.
- Keep modules small, isolated and replaceable (ADR 0010, 0015): by feature, pure cores, a trait only where a
  second implementation exists today. No speculative abstractions.
- Demo data only: code, docs, tests, commits and screenshots never contain the owner's real data
  (`.claude/rules/demo-data.md`). The debug build runs on its own data folder with the demo seed.
- Decisions are ADRs (`docs/adr/`). A record is never edited: a new one supersedes or amends it, and the
  index says so. Status is dated (`Accepted (YYYY-MM-DD)`), the Decision names the alternatives it rejected,
  and a record adopted from a sibling app says so.
- UI changes follow `docs/ux.md` and keep controls stable (`.claude/rules/stable-controls.md`).
- The implementation plan lives in `docs/plan/` (gitignored). Work one step per session; set its status from
  PENDING to DONE when it is committed.
- Changes to the family catalog go through the owner (`app-blueprint`, mode `record`); never edit it from here.

## Docs
Technical docs live in `docs/` (index: `docs/README.md`). Read `docs/architecture.md` and the page of the
layer you change before changing it; `docs/ux.md` is the design language; `docs/adapting.md` lists everything macOS-, name- or WordPress-specific
(for ports, renames and removals). Update the affected docs page in the same commit when behavior,
commands, files or extension points change. Document by file and symbol, never by line number.

## Stack
- Rust backend in `src-tauri/` (rustup stable; Homebrew rustup is keg-only: `/opt/homebrew/opt/rustup/bin`)
- Tauri 2 + React 19 + TypeScript in `src/`, bun as package manager
- macOS 27, Apple Silicon only

## Architecture
- Rust owns all logic; React only renders. Only `api.ts` files call `invoke`/`listen`, one per module
  (`src/<module>/api.ts`; the core's is `src/api/index.ts`). Tests mock them.
- Module map: `docs/architecture.md` → Repository map.
- Frontend state: zustand store in `src/state/store.ts`; components in `src/components/`.
- Errors cross the boundary as `{ kind, message, code }` (`error.rs`); passwords use the redacting `Secret` type.
- Commands live in a Rust catalog (`bus/catalog.rs`, ADR 0036); menus, keys, palette, CLI and MCP run them by id.
- The CLI is the same binary and asks the running app over a Unix socket (ADR 0005).
- No color literal outside `src/styles/schemes/` (ADR 0024); durations are `src/styles/motion.css` tokens
  (ADR 0041); DataBee keeps its editor fonts and schemes (ADR 0035), the rest follows `docs/ux.md`.
- Debug builds: `DataBee (Debug)`, `com.philippstracker.databee.debug`, own data and socket, demo connections,
  dev hooks; never installed (ADR 0028).
- Reads stay in the store (ADR 0034); the only network traffic is to database hosts the user configured
  (ADR 0033); a Dock app whose window hides on close (ADR 0037); a status bar (ADR 0043).

## Commands
Test, lint and format-check commands run through the `ci` agent, never in the main context.
- `bun run test`: Vitest and Rust unit tests
- `bun run test:db`: Docker fixtures (`dev/`) and integration tests (MariaDB 11.4 and MySQL 8.4)
- `bun run lint`: IPC types drift check (`src/api/types.gen.ts`, `src/stage/types.gen.ts`), tsc, ESLint (correctness rules, `eslint.config.js`) and clippy on production code (warnings are errors; Rust tests are not linted)
- `bun run release major|minor|patch`: bumps the version (ADR 0038, 0039), release build, installed by `scripts/install.sh`
  as `~/Applications/DataBee.app` and the `databee` command, committed as "Release X.Y.Z" and tagged `vX.Y.Z`, then `scripts/clean-build.sh` trims `target/`; debug builds ("DataBee (Debug)", a separate app on demo data, ADR 0028) are never installed
- `bun run build:app` + `scripts/dev/launch.sh`: debug build, restarted next to the terminal without stealing focus;
  quit it with `scripts/stop-app.sh <debug CLI>` when the check is done
- `scripts/dev/js.sh` / `scripts/dev/screenshot.sh`: drive and capture the running window in the background;
  `scripts/dev/e2e.sh` checks that nothing jumps, `scripts/dev/screenshot-looks.sh` captures every scheme
- Signing prefers an Apple Development certificate (ADR 0029); without one, `scripts/create-signing-cert.sh`
  (once per Mac) makes the local fallback that keeps privacy grants. Only the `.app` is built, never a `.dmg`
  (ADR 0048).

## Workflow
Finish every step with the `ship-step` skill. Rules in `.claude/rules/`; `testing.md` says which agents in
`.claude/agents/` write and run tests.
Never write to the user's own databases (e.g. DDEV sites); they are read-only test material. Writes in tests
and manual checks go to the Docker fixtures (`wp_write`, `databee_test`).
