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
   taking the instance lock. The handoff is custom: `tauri-plugin-single-instance` is a dependency but unused.
4. **`start_app`** builds Tauri:
   - plugins: log (stdout plus a rotating file, 3 × 5 MiB; `sqlx` is capped at Warn so SQL text stays out of
     the log) and opener;
   - `on_menu_event` → `menu::forward`.

   Then `setup` runs:
   - `AppState::open` loads every JSON store and installs the Keychain secret store.
   - It builds the native menu from the keybindings store.
   - The log level comes from `--log-level`, or else from the settings.
   - It queues the initial launch request. The page pulls it later.
   - It applies the window theme so the vibrancy matches.
   - It generates a Stage token if there is none and starts the Stage server.
   - In debug builds it applies `DATABEE_WINDOW_FRAME` (see [testing](testing.md#driving-the-app)).
   - It binds the IPC socket and serves it in a task.


The window itself is declared in `tauri.conf.json`: label `main`, overlay title bar, transparent background
with the `sidebar` vibrancy effect. No Rust code creates windows.

## AppState

`state.rs`. Tauri manages it, and every command receives it as `State<'_, AppState>`. Each store has its own
mutex inside.

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
| `secrets` | `Box<dyn SecretStore>` | `Keychain` in the app, `MemoryStore` in tests |
| `sessions` | `Sessions` | open database sessions: `RwLock<HashMap<id, Arc<Session>>>` |
| `wp_cache` | `Mutex<HashMap<"session/db", Option<WpContext>>>` | WordPress detection results |
| `config_dir` | `PathBuf` | used to read the optional `entities.toml` |

`stage::Stage` is managed separately (see [stage.md](stage.md)).

## Tauri commands

All commands are registered in one `generate_handler!` in `lib.rs`. Arguments are camelCase on the JS side.
The TypeScript wrappers in `src/api/index.ts` use the same names in camelCase.

| Module | Commands |
|---|---|
| `commands.rs` | `get_settings`, `update_settings` (sanitizes; restarts Stage and applies theme and log level), `get_keybindings`, `set_keybindings`, `get_ui_state`, `set_ui_state`, `open_log_folder`, `open_url` (http/https only), `take_launch_events`, `log_frontend_error`, `list_recents`, `remove_recent` (also forgets that connection's UI state), `forget_password`, `list_favorites`, `create_favorite`, `update_favorite`, `remove_favorite`, `reorder_favorites`, `list_history`, `clear_history` |
| `db_commands.rs` | `connect`, `disconnect`, `list_databases`, `list_tables`, `describe_table`, `completion_schema`, `use_database`, `execute`, `cancel_query`, `browse_table` (see [database.md](database.md)) |
| `wp_commands.rs` | `wp_detect`, `wp_dashboard`, `wp_browser`, the list commands (`wp_orders`, `wp_users`, `wp_products`, `wp_actions`, `wp_sessions`, `wp_content`), `wp_view`, `wp_search`, `wp_peek`, `wp_find_value`, options and snapshots, `plan_change`, `apply_changes`, `edit_serialized`, `decode_value` (see [wordpress.md](wordpress.md)) |
| `stage_commands.rs` | `stage_anchors`, `stage_done`, `stage_info` (the page's replies), `stage_status`, `stage_new_token` |

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
A password from the CLI stays in Rust. The page calls `connect` with the `requestId`, and Rust looks the
password up.

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
- `IpcMessage`'s `Debug` redacts the password.
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

`secrets.rs` defines the `SecretStore` trait (`get`, `set`, `delete`) and implements it twice:

- `Keychain`: generic passwords with service `"databee"` and account `ConnectionSpec::account()`
  (`user@host:port`), so one password covers every database on a server.
- `MemoryStore`: for tests.

`resolve_password` picks the first available source: an explicit password, then the CLI password for the
launch request, then the stored one.

`connection.rs` holds `ConnectionSpec` (`name`, `host`, `port`, `socket`, `user`, `database`; never a password)
and `Secret`. `Secret` prints as `Secret(***)`, cannot be serialized, and gives up its value only through
`expose()`.

## Errors

`error.rs`: `AppError` serializes as `{ kind, message, code }`, where `message` is the `Display` text.

| kind | Raised for |
|---|---|
| `database` | server errors; `code` is the MySQL error number |
| `connection` | IO/TLS failures, pool timeouts, connect timeouts |
| `auth_failed` | MySQL error 1045 |
| `password_required` | no password was available |
| `timeout` | the query exceeded the timeout |
| `cancelled` | MySQL error 1317, or the user cancelled |
| `confirmation_required` | a destructive statement without `confirmed` |
| `session_not_found` | a stale session id |
| `invalid_input` | bad arguments, a bad filter, a bad `entities.toml` |
| `keychain` | Keychain failures |
| `io` | file errors |

The frontend mirrors this type as `AppError` in `src/api/types.ts`.

## Native menu

`menu.rs` builds the menus once, at startup: DataBee, File, Edit, View, Go and Window. Items that map to
app commands use the ids listed in `LABELS`. Their accelerators come from
`KeybindingsStore::menu_binding`: the user override, else `MENU_DEFAULTS` in `store/keybindings.rs`. Those
defaults mirror the `defaultBinding` of the TypeScript commands marked `menu: true`. A click emits `command`
with the id. `keybindings::accelerator` converts a binding to Tauri's form (`Mod` → `CmdOrCtrl`, `-` →
`Minus`, `=` → `Equal`). Tauri accelerators name physical keys and have none for `+`, so `+` becomes
`Shift+Equal` (its US-layout key); a key without an accelerator name (e.g. `Ä`) gets no menu accelerator,
while the webview still handles it.

To add a menu item:
1. Set `menu: true` on the TypeScript command.
2. Add the id to `LABELS` in `menu.rs`.
3. Place the item in `build()`.
4. Add the default to `MENU_DEFAULTS`.

The menu isn't rebuilt when a binding changes, so a new menu accelerator takes effect after a restart.

## Dependencies

| Crate | Use |
|---|---|
| `tauri` (feature `macos-private-api`), `tauri-build` | app framework; the feature allows a transparent window |
| `tauri-plugin-log`, `log` | logging |
| `tauri-plugin-opener` | open URLs and the log folder |
| `tauri-plugin-single-instance` | unused (the custom socket replaces it) |
| `sqlx` (`mysql`, `mysql-rsa`, no TLS) | MySQL/MariaDB driver |
| `tokio` | runtime, IPC socket, Stage HTTP server |
| `serde`, `serde_json` (`preserve_order`) | IPC, stores, JSON values in their stored key order |
| `clap` | CLI |
| `security-framework` | macOS Keychain |
| `thiserror` | `AppError` |
| `chrono`, `uuid`, `url`, `percent-encoding`, `futures-util` | time stamps, ids, URL parsing, result streaming |
| `toml` | `entities.toml`, `meta_keys.toml` |
| `sha2` | Gravatar hashes |
| `libc` | `getuid` for the IPC peer check |
| `tempfile` (dev) | store and socket tests |

The release profile uses LTO, `codegen-units = 1`, `panic = "abort"` and strip. The toolchain is pinned to
stable in `rust-toolchain.toml` at the repo root. The crate uses edition 2024 and let-chains.
