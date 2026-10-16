# Backend (Rust core)

This page covers `src-tauri/`, the Rust side of DataBee. The database driver, the WordPress lens and the
Stage hook each have their own page.

## Startup

`main.rs` calls `databee_lib::run()` (`lib.rs`):

1. **Config dir.** `config_dir()` resolves `~/Library/Application Support/<identifier>` (no home dir: exit 2)
   and `create_private_dir` (`store/json_file.rs`) makes it mode 0700. This happens before Tauri starts, because the CLI and the
   socket need the path first. The CLI reads `default_port` with `Settings::peek`, which never quarantines a
   corrupt file.
2. **CLI.** `cli::parse` returns one of three outcomes:
   - `Exit`: `--help` or `--version`.
   - `Run { launch, log_level }`: an `IpcMessage::Launch` with a connection, or `Activate` without one.
   - An error: it becomes `IpcMessage::Error`, and the app shows it.
3. **Single-instance handoff.** `ipc::try_forward` sends the message to a running instance (see
   [below](#cli-and-single-instance-handoff)). Only when no instance listens does startup continue, after
   taking the instance lock. The handoff is custom (no `tauri-plugin-single-instance`).
4. **`AppState`.** `run` opens every JSON store with `AppState::open`, installs the Keychain secret store and
   keeps the `--log-level` override in `AppState::log_override`. The effective level (override, else the
   stored `log_level`) is computed here, so logging never starts at `Trace`.
5. **`start_app`** builds Tauri, with `.manage(state)` as the first builder call so plugins can use it:
   - plugins: log (stdout plus a rotating file, 3 × 5 MiB; `sqlx` is capped at Warn so SQL text stays out of
     the log), `init_logging` (a one-line plugin that sets the effective level right after the log plugin's
     setup, which sets `Trace` because its dispatch level caps every later change) and opener;
   - `on_menu_event` → `menu::forward`.

   Then `setup` runs:
   - It builds the native menu from the keybindings store.
   - It queues the initial launch request. The page pulls it later.
   - `ensure_stage_token` generates a Stage token if there is none. A token that can't be saved is logged
     and kept in memory.
   - It manages `Stage` and calls `commands::apply_settings(app, None, &settings)`: window theme
     (`window::apply_window_theme`), log level and the Stage server.
   - In debug builds `apply_dev_frame` applies `DATABEE_WINDOW_FRAME` (see [testing](testing.md#driving-the-app)).
   - `start_ipc` binds the IPC socket and serves it in a task; every message goes to
     `launch::on_ipc_message`. A bind error is logged and the app runs without single-instance handoff.

### Settings side effects

`update_settings` and startup both call `commands::apply_settings(app, previous, saved)`. The decision is the
pure `settings_effects(previous, saved, log_override)`: without `previous` everything applies; otherwise only
what changed (theme, log level, Stage settings). A CLI `--log-level` wins for the app's lifetime, so a
settings change never resets it.

The window itself is declared in `tauri.conf.json`: label `main`, overlay title bar, transparent background
with the `sidebar` vibrancy effect. No Rust code creates windows.

## AppState

`state.rs`. Tauri manages it, and every command receives it as `State<'_, AppState>`. Each store has its own
mutex inside. Panics unwind ([ADR 0017](adr/0017-panic-unwind.md)), so a panic while a guard is held poisons a
`std::sync` lock: code takes these locks with `lock::MutexExt::lock_unpoisoned` and
`lock::RwLockExt::{read_unpoisoned, write_unpoisoned}` instead of `unwrap()`, and keeps working with the data.
`AppState` holds no module data: per-session data of a module lives in `Session.ext`
([ADR 0013](adr/0013-session-extensions.md)).

| Field | Type | Holds |
|---|---|---|
| `settings` | `SettingsStore` | `settings.json` |
| `keybindings` | `KeybindingsStore` | `keybindings.json`; also feeds the menu |
| `recents` | `RecentsStore` | `connections.json` (last 20 connections) |
| `favorites` | `FavoritesStore` | `favorites.json` |
| `history` | `HistoryStore` | `history.json` (last 500 queries) |
| `snapshots` | `SnapshotStore` | `snapshots.json` (WordPress snapshots) |
| `ui_state` | `UiStateStore` | `ui_state.json` (column widths, pane sizes per connection) |
| `launches` | `LaunchQueue` | pending launch events and one-shot passwords by request id |
| `secrets` | `Arc<dyn SecretStore>` | shared into blocking tasks (the Keychain may block on a dialog); `Keychain` in the app, `MemoryStore` in tests (compiled only with `cfg(test)` or the `test-util` feature) |
| `sessions` | `Sessions` | open database sessions: `RwLock<HashMap<id, Arc<Session>>>` |
| `config_dir` | `PathBuf` | used to read the optional `entities.toml` |
| `log_override` | `Option<LogLevel>` | the CLI `--log-level`; wins over the stored level (`settings_effects`) |

`stage::Stage` is managed separately (see [stage.md](stage.md)).

## Tauri commands

All commands are registered in one `generate_handler!` in `lib.rs`. Arguments are camelCase on the JS side.
The TypeScript wrappers in `src/api/index.ts` use the same names in camelCase.

| Module | Commands |
|---|---|
| `commands.rs` | `get_settings`, `update_settings` (sanitizes; restarts Stage and applies theme and log level), `get_keybindings`, `set_keybindings`, `get_ui_state`, `set_ui_state`, `open_log_folder`, `open_url` (http/https only, no credentials in the URL), `take_launch_events`, `log_frontend_error` (truncated to 4 KiB, line breaks escaped), `list_recents`, `remove_recent` (also forgets that connection's UI state), `list_favorites`, `create_favorite`, `update_favorite`, `remove_favorite`, `reorder_favorites`, `list_history`, `clear_history` |
| `db_commands.rs` | `connect` and `disconnect` (thin wrappers over `session::open` / `session::close`), `list_databases`, `list_tables`, `describe_table`, `completion_schema`, `use_database`, `execute`, `cancel_query`, `browse_table` (see [database.md](database.md)) |
| `wp_commands.rs` | `wp_detect`, `wp_dashboard`, `wp_browser`, the list commands (`wp_orders`, `wp_users`, `wp_products`, `wp_actions`, `wp_sessions`, `wp_content`), `wp_view`, `wp_search`, `wp_peek`, `wp_find_value`, options and snapshots, `plan_change`, `apply_changes`, `edit_serialized`, `decode_value` (see [wordpress.md](wordpress.md)) |
| `stage_commands.rs` | `stage_anchors`, `stage_done`, `stage_info` (the page's replies), `stage_status`, `stage_new_token` |

`session::open` is the connect service: it peeks the launch password, resolves the password
(`resolve_password`, in `spawn_blocking` because the Keychain may block on a dialog), calls `db::open`, maps a
rejected login through the pure `auth_error`, saves a typed password to the Keychain when asked, touches
recents and registers the session. `SessionInfo::new` is pure and carries `identity`, `account` (the Keychain
account that also scopes favorites) and `displayName`, so the page never derives them. `session::close`
removes the session and calls `Database::close`, which interrupts a running query and closes the editor
connection and the pool. `db_commands::run_query` holds `execute`'s logic (confirmation gate before the
session lookup, history entry for failed queries too).

**Adding a command:**
1. Write a `#[tauri::command]` function that returns `AppResult<T>`. `T` must be `Serialize` with
   `#[serde(rename_all = "camelCase")]`.
2. Register it in `lib.rs`.
3. Add a wrapper to `api` in `src/api/index.ts`, and the types to `src/api/types.ts`.
4. Add a mock to `src/test/apiMock.ts`. TypeScript requires it, because the mock is typed as `typeof api`.

## Events to the page

| Event | Payload | Emitted when |
|---|---|---|
| `launch` | none | a second process handed over a request; the page then calls `take_launch_events` |
| `command` | command id, e.g. `"tabs.new"` | a native menu item was clicked |
| `stage:collect`, `stage:act`, `stage:info` | `{ request, … }` | a Stage request needs the page ([stage.md](stage.md)) |

A `LaunchEvent` is `{type:"connect", requestId, spec, hasPassword, savePassword}` or `{type:"error", message}`.
`launch.rs` defines the event name (`LAUNCH_EVENT`). A password from the CLI stays in Rust. The page calls
`connect` with the `requestId`, and Rust looks the password up.

## CLI and single-instance handoff

`cli.rs` (clap derive):

- **URL:** `mysql://` or `mariadb://user[:pw]@host[:port][/db][?socket=path]`, percent-decoded.
- **Flags:** `-H/--host`, `-P/--port`, `-S/--socket`, `-u/--user`, `-d/--database`, and `-n/--name` for the UI
  label. Flags override the parts of the URL.
- **Password sources:** `--password`, `--password-stdin` or `--password-env VAR`. The three are mutually
  exclusive and win over a password in the URL. `--save-password` stores it in the Keychain after a successful
  connect.
- **Logging:** `--log-level` overrides the setting.
- **Defaults:** host `127.0.0.1`, user `root`, port from `settings.defaultPort`.
- **Activate only:** without any connection argument (URL, host, port, socket, user, database, name or a
  password flag), the process only activates the running window.
- **URL details:** IPv6 hosts lose their brackets; the path names at most one database; an empty `?socket=`
  counts as none. `--password-stdin` on empty stdin is an error.
- **Argv caveat:** a forwarded launch keeps its password out of the running app's argv, but the process that
  becomes the app keeps its own argv, so a password in its URL shows in `ps` for the whole session.

`ipc.rs`:

- **Socket:** a Unix domain socket at `<config dir>/databee.sock`, with mode 0600 in a 0700 dir. `serve` drops
  peers with another uid.
- **Protocol:** one line of JSON per connection (at most 64 KiB, within 5 s), answered with `ok\n`. Malformed,
  oversized or incomplete lines get no answer.
- **`Forward` outcomes** of `try_forward`, and what `run` does with them:
  - `Delivered`: exit 0 (2 for a CLI error);
  - `NoInstance` (socket missing or refused): start the app;
  - `Rejected` (no `ok`) or an error such as a timeout: print it and exit 2, so a hung instance never
    leads to a second one.
- **Instance lock:** before starting, `run` takes an exclusive `flock` on `<config dir>/databee.lock` and
  holds it for the process lifetime. A launch that finds it held polls `try_forward` for up to 5 s while
  the holder starts up.
- **Messages** (`IpcMessage`, tagged by `type`):
  - `launch {spec, password, savePassword}`
  - `error {message}`
  - `activate`
  - `eval {js}`: compiled into debug builds only, used by `scripts/dev/js.sh`.
- **Handling** (`launch::on_ipc_message`): `eval` runs in the window; everything else is logged by type
  (`IpcMessage::kind`, never the contents), goes through `LaunchQueue::push`, emits `launch` when it queued an
  event, then unminimizes and focuses the window (not with `DATABEE_BACKGROUND=1`).
- The launch `password` is a `Secret`: plain string on the wire (`connection::serialize_exposed`), redacted in
  `IpcMessage`'s derived `Debug`.
- **Stale sockets:** `bind` removes a leftover socket only when connecting to it is refused; it refuses to
  bind while a listener is alive.

## Persistence

Everything lives in the config dir, `~/Library/Application Support/com.philippstracker.databee/`. Every JSON
store wraps a `JsonStore<T>` (`store/json_store.rs`): the parsed value behind one lock, with `read(|t| ..)`
and `update(|t| ..) -> AppResult<R>`. `update` applies the closure to a clone, saves the clone and only then
replaces the value, holding the lock throughout, so a failed save leaves memory unchanged instead of
showing a change that vanishes on restart. `JsonStore::open_with` normalizes the loaded value (settings
use it for `Settings::sanitized`). The file itself is `JsonFile` (`store/json_file.rs`), which:

- writes pretty-printed JSON to `<name>.tmp` (mode 0600), `sync_all`s it and renames it over the target;
  on error the temp file is removed. The parent dir is created 0700 with `create_private_dir`;
- loads defaults when a file is missing;
- moves any other file it cannot read or parse (permission denied, invalid UTF-8, invalid JSON) aside to
  `<name>.<UTC yyyymmddTHHMMSS>.corrupt` (with a `-<n>` counter when taken, so earlier backups survive) and
  logs a warning, so a bad edit never blocks startup. If that rename fails it logs an error and marks
  the file poisoned: `save` then refuses with `AppError::Io` instead of overwriting the user's file.

| File | Contents |
|---|---|
| `settings.json` | `Settings` (camelCase, `#[serde(default)]`). `sanitized()` clamps values on load and on update: font sizes, row limit, timeouts, ports. Renamed fields use serde aliases. Enum fields (`theme`, `density`, `logLevel`, `inspectorDock`) use `deserialize_with = "lenient"`: an unknown value falls back to that field's default instead of quarantining the file. `SettingsStore::update` returns `(previous, saved)` from one lock. |
| `keybindings.json` | `{ "commandId": "Mod+Shift+X" \| null }` overrides; `null` unbinds. `keybindings::sanitize` runs on load and in `KeybindingsStore::update`: it drops (and logs) entries whose id is not `[A-Za-z0-9.]+` or whose binding is malformed (unknown modifier, missing or multi-part key). A trailing `++` is the `+` key (`Mod+Shift++`) |
| `connections.json` | recent connections by identity (`user@host:port/db`) |
| `favorites.json` | tables and saved queries, global or scoped to one server account. `FavoritesStore::create(NewFavorite)` assigns the id; `update(Favorite)` replaces by id and refuses an unknown id. Both refuse a blank name and a second favorite with the same `(scope, target)` (`invalid_input`) |
| `history.json` | executed queries per account, with time, duration and error. At most 500 entries across all accounts. SQL over 64 KiB is stored cut on a char boundary with `truncated: true`. Each entry stores an FNV-1a `hash` of the full SQL (computed on load for older files); rerunning the same SQL on the same account replaces its entry |
| `snapshots.json` | WordPress snapshots, 30 per account and database (`MAX_PER_DATABASE`); adding one evicts the oldest of that database only |
| `ui_state.json` | per connection: column widths, `detailPaneWidth`, `inspectorWidth`, `bottomPaneHeight`, `sqlEditorHeight` (older names load via serde aliases). `UiStateStore::forget` drops a connection's entry |
| `entities.toml` | optional user entity definitions ([wordpress.md](wordpress.md#entities)) |
| `databee.sock` | the IPC socket |
| `databee.lock` | the instance lock |

Logs go to Tauri's `app_log_dir`, which is `~/Library/Logs/com.philippstracker.databee/`.

**Adding a setting:**
1. Add the field to `Settings` in `store/settings.rs`, with a default.
2. Clamp it in `sanitized`; give an enum field `#[serde(deserialize_with = "lenient")]`.
3. Mirror it in `src/api/types.ts` and `src/test/settings.ts`.
4. Render it in `SettingsDialog.tsx`.

## Secrets

`secrets/mod.rs` defines the `SecretStore` trait (`get`, `set`, `delete`) and `resolve_password`; each
implementation has its own file:

- `keychain.rs`, `Keychain`: the only file that imports `security_framework`. Generic passwords with service `"databee"` and account `ConnectionSpec::account()`
  (`user@host:port`, or `user@socket`), so one password covers every database on a server. `%`, `@` and `:` in
  the user are percent-encoded, so `a@b` on `c` and `a` on `b@c` get different entries; other users keep
  their plain name.
- `memory.rs`, `MemoryStore`: for tests, behind `#[cfg(any(test, feature = "test-util"))]`. The crate lists
  itself with `test-util` as a dev-dependency, so integration tests in `tests/` can use it
  ([ADR 0019](adr/0019-test-doubles-behind-test-util.md)); release builds never contain it.

The implementation is chosen in exactly one line, `Arc::new(secrets::Keychain)` where `run` builds `AppState`
in `lib.rs`. To use another store, see [adapting.md](adapting.md#swap-the-secret-store).

`resolve_password` returns a `PasswordSource`: `Provided` (the page's password, else the CLI password for the
launch request), `Stored` (Keychain), `None`, or `Unavailable(reason)` when the Keychain lookup failed.
`connect` then tries without a password; on an auth failure `db_commands::auth_error` maps the source to the
error the UI prompts with (`Unavailable` and a rejected `Stored` password name the cause).

One-shot CLI passwords live in `LaunchQueue` by request id and expire after 5 minutes (pruned on every
`push`, `peek_password` and `take_password`). `connect` always peeks the launch password when a `requestId` is
given, and consumes it only on success or an auth failure, so a timeout or network error leaves it for a
retry.

`connection.rs` holds `ConnectionSpec` (`name`, `host`, `port`, `socket`, `user`, `database`; never a password)
and `Secret`. `Secret` wraps `Zeroizing<String>` (zeroed on drop), prints as `Secret(***)`, deserializes from a
string, has no `Serialize` impl (only `serialize_exposed` for the IPC wire format), compares only in tests, and
gives up its value only through `expose()`.

## Errors

`error.rs`: `AppError` serializes as `{ kind, message, code, account }`, where `message` is the `Display` text and
`account` names the Keychain account of a login error (`null` otherwise), so the password prompt never derives it.

| kind | Raised for |
|---|---|
| `database` | server errors; `code` is the MySQL error number, or the SQLSTATE when there is none |
| `connection` | IO/TLS/protocol failures, a crashed connection worker, pool timeouts or a closed pool, connect timeouts, and MySQL errors 1053, 1927, 2006, 2013, 4031 (the connection is gone, so `execute` reconnects) |
| `auth_failed` | MySQL error 1045 (the message is the server's); on connect `LoginFailed`, explained by the password source |
| `password_required` | no password was available |
| `timeout` | the query exceeded the timeout; the message shows `500 ms` below one second, else whole seconds |
| `cancelled` | MySQL error 1317, or the user cancelled |
| `confirmation_required` | a destructive statement without `confirmed` |
| `session_not_found` | a stale session id |
| `invalid_input` | bad arguments, a bad filter, a bad `entities.toml` |
| `keychain` | Keychain failures |
| `io` | file errors |

The frontend mirrors this type as `AppError` in `src/api/types.ts`.

Logging of failures: a failed query logs `kind` and `code` at Info and the message (which may quote SQL) at
Debug; the SQL itself only at Trace. Text from `log_frontend_error` is cut to 4 KiB and its `\n`/`\r` are
escaped, so the page cannot forge log lines.

## Native menu

`menu.rs` builds the menus: DataBee, File, Edit, View, Go and Window. Items that map to app commands are listed
once in `MENU_ITEMS` (id, label, default binding). Their accelerators come from
`KeybindingsStore::menu_binding`: the user override, else the `MENU_ITEMS` default. Those defaults mirror the
`defaultBinding` of the TypeScript commands marked `menu: true` (a unit test in `menu.rs` compares them). A
click emits `command` with the id (`menu::forward`; `is_menu_command` filters out the native items).
`keybindings::accelerator` converts a binding to Tauri's form (`Mod` → `CmdOrCtrl`, `-` → `Minus`, `=` →
`Equal`). Tauri accelerators name physical keys and have none for `+`, so `+` becomes `Shift+Equal` (its
US-layout key); a binding without an accelerator (e.g. `Ä`) is logged and the item gets no menu accelerator,
while the webview still handles it.

To add a menu item:
1. Set `menu: true` (and `defaultBinding`) on the TypeScript command.
2. Add the id, label and default to `MENU_ITEMS` in `menu.rs`.
3. Place the item in `build()`.

`set_keybindings` rebuilds the menu (`menu::rebuild`), so a new accelerator applies at once.

## Dependencies

| Crate | Use |
|---|---|
| `tauri` (feature `macos-private-api`), `tauri-build` | app framework; the feature allows a transparent window |
| `tauri-plugin-log`, `log` | logging |
| `tauri-plugin-opener` | open URLs and the log folder |
| `sqlx` (`mysql`, `mysql-rsa`, no TLS) | MySQL/MariaDB driver |
| `tokio` | runtime, IPC socket, Stage HTTP server |
| `serde`, `serde_json` (`preserve_order`) | IPC, stores, JSON values in their stored key order |
| `clap` | CLI |
| `security-framework` | macOS Keychain |
| `zeroize` | `Secret` is zeroed on drop |
| `thiserror` | `AppError` |
| `chrono`, `uuid`, `url`, `percent-encoding`, `futures-util` | time stamps, ids, URL parsing, result streaming |
| `toml` | `entities.toml`, `meta_keys.toml` |
| `sha2` | Gravatar hashes |
| `libc` | `getuid` for the IPC peer check |
| `tempfile` (dev) | store and socket tests |

The library builds as `rlib` only (the `staticlib`/`cdylib` types of the Tauri template are for mobile). The
release profile uses LTO, `codegen-units = 1` and strip, and keeps the default `panic = "unwind"`: Tauri and
tokio catch a panic per command or task, so one bug fails one call instead of killing the process with the
user's open transactions ([ADR 0017](adr/0017-panic-unwind.md)). The toolchain is pinned to
stable in `rust-toolchain.toml` at the repo root. The crate uses edition 2024 and let-chains.

## Content security policy

`app.security.csp` in `tauri.conf.json` limits what the webview may load, because it renders arbitrary database
content:

- `default-src 'self'`: scripts, fonts and frames come from the bundle only;
- `img-src 'self' data: https: http:`: Gravatar avatars and WordPress thumbnails, which come from the site's own
  uploads URL (any local host, such as `*.ddev.site`);
- `style-src 'self' 'unsafe-inline'`: React `style` attributes;
- `connect-src ipc: http://ipc.localhost`: Tauri IPC (commands and events).

`app.security.devCsp` applies under `tauri dev` and adds inline scripts (the React refresh preamble) and Vite's
HMR websocket on `localhost:1420`/`1421`.
