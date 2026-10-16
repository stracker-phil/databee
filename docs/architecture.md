# Architecture

DataBee is a single-window desktop app built with Tauri 2. A Rust process owns every piece of logic:
connections, SQL, WordPress detection, persistence, secrets, and the CLI. A React 19 webview renders
state and forwards user intent. This page covers the overall shape. The layer pages go into detail:
[backend](backend.md), [database](database.md), [WordPress](wordpress.md), [frontend](frontend.md),
[Stage hook](stage.md).

## Design rules

These rules hold everywhere in the code. Keep them when you change things.

1. **Rust owns the logic; React only renders.** SQL is built in Rust, and so are planned edits, WordPress
   parsing and safety checks. The frontend never assembles a write statement on its own. It asks Rust to
   *plan* a change, shows the planned SQL, and sends it back to be applied.
2. **One IPC boundary.** `src/api/index.ts` is the only module that calls `invoke` or `listen`. The rest of the
   frontend imports `api` from there, which is also what UI tests mock.
3. **Errors are data.** Every command returns `Result<T, AppError>`, which serializes as `{ kind, message, code, account }`.
   The UI branches on `kind` (e.g. `password_required`, `confirmation_required`).
4. **Passwords never touch files, logs or the webview.** They live in the `Secret` type. It redacts in
   `Debug`, is zeroed on drop and has no `Serialize` impl. Passwords are stored in the OS keychain and handed to a connect call by
   request id.
5. **Show what is stored.** Values travel losslessly: numbers and dates as strings, binary values as a hex
   preview, PHP-serialized data byte for byte. Labels may be shown next to a raw value, never instead of it.
   The WordPress layer has extra rules in [`.claude/rules/wp-layer.md`](../.claude/rules/wp-layer.md).
6. **No silent writes.** Grid edits, WordPress edits and maintenance actions all go through the same
   *plan → review → apply in one transaction → undo* pipeline. Destructive SQL typed in the editor asks
   for confirmation first.

## Process model

```
┌──────────── DataBee.app ─────────────────────────────────────────────┐
│                                                                      │
│  Webview (React)                       Rust core (tokio runtime)     │
│  ┌──────────────────────┐   invoke    ┌───────────────────────────┐  │
│  │ components/          │ ──────────▶ │ #[tauri::command]s        │  │
│  │ state/store.ts       │             │  commands.rs  db_commands │  │
│  │ commands/ (registry) │ ◀────────── │  wp_commands  stage_cmds  │  │
│  │ api/index.ts ◀── only│   events    ├───────────────────────────┤  │
│  └──────────────────────┘ launch,     │ AppState: stores, sessions│  │
│                           command,    │ db/ (dyn Database)       │  │
│                           stage:*     │ wp/ (lens)  store/ (JSON) │  │
│                                       │ secrets (Keychain)        │  │
│                                       └──────┬───────┬────────┬───┘  │
└──────────────────────────────────────────────┼───────┼────────┼──────┘
                         Unix socket (ipc.rs)  │       │ sqlx   │ HTTP 127.0.0.1
   `databee …` (2nd process) ──────────────────┘       ▼        ▼ (stage/)
                                                MySQL/MariaDB   Stage presenter app
```

- **One instance.** The first `databee` process binds a Unix socket in the config directory. Later launches,
  such as `databee mysql://…` from a script, send their parsed request over that socket and exit
  ([backend: CLI and IPC](backend.md#cli-and-single-instance-handoff)).
- **Launch requests are pulled, not pushed.** Rust queues launch requests and emits a bare `launch` event. The
  page calls `take_launch_events`. The page can't miss the initial request even though it subscribes late.
- **The native menu emits `command` events** carrying a command id. The page runs that id through the same
  registry that handles shortcuts and the palette.
- **The Stage hook** is an optional local HTTP API through which a presenter app drives the UI
  ([stage.md](stage.md)).

## Repository map

```
src-tauri/                 Rust crate `databee` (lib `databee_lib`)
  src/lib.rs               startup, plugin setup, command registration, run loop
  src/cli.rs ipc.rs        argv parsing, single-instance socket
  src/launch.rs            queue of launch requests + one-shot passwords
  src/state.rs session.rs  AppState, open sessions
  src/commands.rs          settings, keybindings, favorites, history, misc
  src/db_commands.rs       connect, schema, execute, browse
  src/wp_commands.rs       WordPress lens + plan/apply of changes
  src/stage_commands.rs    Stage page replies and settings controls
  src/db/                  MySQL driver, SQL lexer, classify, safety, cell decoding
  src/decode/              PHP serialize and JSON value trees, leaf edits
  src/wp/                  WordPress detection, entities, views
  src/store/               JSON stores in the config dir
  src/stage/               Stage HTTP API and request hub
  src/secrets/             SecretStore trait; Keychain and test MemoryStore
  src/menu.rs              native menu (MENU_ITEMS)
  src/window.rs            window theme and bring-to-front
  src/error.rs connection.rs  AppError, ConnectionSpec, Secret
  src/lock.rs              poison-tolerant std lock access
  tests/                   integration tests (Docker, #[ignore])
  tauri.conf.json          window, bundle and identifier config
src/                       React app
  api/                     the IPC boundary + types generated from Rust (ts-rs)
  state/                   zustand store: store.ts (composition root), slices/, selectors, session events
  commands/                command registry, scopes, default shortcuts
  registry/                extension points modules register into (settingsPages.ts)
  components/              UI; components/wp/ is WordPress mode, components/settings/ the Settings dialog
  stage/                   page side of the Stage hook, its Settings page, installStage()
  lib/                     pure helpers (formatting, SQL ranges, theme)
  styles/                  theme.css (tokens) + app.css (components)
  test/                    api mock, fixtures
dev/                       Docker fixtures for integration tests
scripts/                   install links; dev/ drives the app headlessly
.claude/                   rules and the ship-step skill for Claude Code
```

## A request, end to end

Running `SELECT * FROM wp_options` in a query tab:

1. `QueryTab` calls `store.runQuery(tabId, sql)`.
2. The store calls `api.execute(sessionId, sql, false)`, which wraps `invoke("execute", …)`.
3. `db_commands::execute` looks up the `Session`. If `confirm_destructive` is on, it checks the SQL with
   `db::safety::destructive_reason` and returns `ConfirmationRequired` for statements like `DROP` or an
   unscoped `DELETE`.
4. `Database::execute` (`MySqlSession`) runs the whole script on the session's dedicated editor connection with the
   configured timeout and row limit. It decodes each cell into a typed `Cell` and returns one `ResultSet`
   per statement.
5. Rust records a history entry and returns `ExecuteResponse` to the store. If an error has kind
   `confirmation_required`, the store opens a confirm dialog and re-runs with `confirmed = true`.
6. `ResultsGrid` renders the last result set, virtualized.

Editing a cell is a separate round trip. `queueChange` calls `plan_change`, which reads the current value and
returns `{description, statements, undo}`. The plan appears in the pending tray, and ⌘S calls
`apply_changes`, which runs every pending statement in one transaction. Undo replays the `undo` statements.

## Platform

The code base targets **macOS on Apple Silicon** only. The platform-specific parts are listed in
[adapting.md](adapting.md#platform-specific-code): the Keychain, the config path, Unix sockets, window vibrancy
and the dev scripts. Most of the code is portable: the database layer, the WordPress layer, the stores and
nearly all of the UI.
