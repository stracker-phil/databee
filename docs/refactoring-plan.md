# DataBee: full code review and refactoring plan

## Context

DataBee grew from a SQL client into a client plus a WordPress lens plus a Stage presentation hook.
The code works, but it accumulated large modules (`wp/detail.rs` 2162 lines, `stage/hooks.rs` 1405,
`wp/lists.rs` 1342, `store.ts` 898, `api/types.ts` 816), commands that hold logic, duplicated SQL
building, stores that mutate memory before saving, and optional features (Stage, WordPress, Keychain)
wired into the core in many places.

Goal of this plan: turn the app into a set of modules with one install point each, so a module can be
removed (delete its folder, drop one line) or swapped (implement its trait, change one line). Along
the way fix the correctness, security and performance problems found in the review, cut comments to
what explains "why", and give every unit observable-behavior tests.

This document has two parts:
1. **Findings**: every problem found, grouped by layer and file, with the concrete fix.
2. **Plan**: ordered steps, each small enough for one commit via `ship-step`, with the files, symbols
   and tests involved. Steps are written so an agent without the review context can execute them.

Conventions for the implementer: follow `CLAUDE.md` and `.claude/rules/`; tests are written by the
`write-ts-unit-test` / `write-rust-unit-test` agents and run by the `ci` agent; every step ends with
`ship-step`. Never touch the user's DDEV databases.

---

# Part 1: Findings

Severity tags: [bug] [security] [design] [perf] [comment] [test] [product].

## 1. Rust core (`lib.rs`, `cli`, `ipc`, `launch`, `state`, `session`, `connection`, `commands`, `db_commands`, `error`, `secrets`, `dev`, `menu`, `db/`, `store/`, `tests/`)

### lib.rs
- `config_dir` [security]: no home dir falls back to `/tmp/Library/...`, world-writable. Exit with an error instead.
- `run` [bug]: `SettingsStore::open` is called only to read `default_port`, and `open` renames a corrupt `settings.json` to `.corrupt`. A second short-lived `databee mysql://…` process can move the running instance's file aside. Add a read-only `Settings::peek(dir)`.
- `run` [bug]: `try_forward` `Err` (hung instance, 5 s timeout) or `Ok(false)` (no ack) both continue to `start_app`, which unlinks the live socket; two instances run. Only start on "no listener" (NotFound / ConnectionRefused); otherwise print the error and exit non-zero.
- `run` [bug]: two simultaneous launches both see no socket and both start. Take an `flock` on `<config>/databee.lock` before binding.
- `start_app` setup closure [design]: ~50 lines doing state, menu, log level, launch queue, theme, Stage token, dev frame, IPC. Split into `init_logging`, `apply_dev_frame`, `start_ipc(handle, socket)`, and move Stage out entirely (see Part 2, step S1).
- IPC handler closure [design]: message routing (eval, push, emit, focus) is inline and untestable. Move to `launch::on_ipc_message(&AppHandle, IpcMessage)`; keep the queue decision (`LaunchQueue::push`) separate from window side effects.
- `start_app` [bug]: `state.settings.update(settings)?` and `ipc::bind(&socket)?` abort startup on a read-only config dir or socket error. Log and continue (token in memory, no single-instance).
- `start_app` [bug]: Stage server starts before `app.manage(state)`; manage first.
- `start_app` [security]: `IpcMessage::Eval` only ignored via `cfg!(debug_assertions)` at runtime; gate the variant with `#[cfg(debug_assertions)]` so release builds refuse to deserialize it.
- `start_app` [bug]: `let _ =` on `w.eval`, `handle.emit(LAUNCH_EVENT)`, `unminimize`, `set_focus`, `set_position`, `set_size`. A failed emit means a launch request never reaches the page. Log `emit` and `eval` failures with `log::warn!`.
- `start_app` [comment]: "Received launch request from another process" also logs for `Activate`/`Error`. Log the message type.
- `LAUNCH_EVENT` [design]: lives in `lib.rs` while `COMMAND_EVENT` lives in `menu.rs`. Move next to `LaunchQueue`.
- `IDENTIFIER` [design]: duplicates `identifier` in `tauri.conf.json` unchecked. Add a unit test that reads the JSON and compares.
- `log_plugin` [design]: level is `Trace` until `setup` lowers it. Pass the initial level from `run`.
- `stage::new_token` [comment]: two v4 UUIDs are 244 random bits, not "256". Use 32 bytes from `getrandom` (or fix the doc).

### main.rs
- [comment] `// Prevents additional console window on Windows in release, DO NOT REMOVE!!` plus the `windows_subsystem` attribute: template leftovers on a macOS-only app. Delete both.

### state.rs
- `AppState` [design]: public bag of fields; `wp_cache` keyed by `"session/db"` is per-session data held globally. Move the lens cache into `Session`.
- `wp_cache` [bug]: `disconnect` never clears it; grows for the process lifetime. Fixed by moving it into `Session`.
- [test]: none. `AppState::open(tempdir, Box::new(MemoryStore::default()))` opens every store on an empty dir; corrupt files don't panic.

### session.rs
- `Sessions::remove` / `MySqlSession::close` [bug]: `close()` closes only the pool; the editor connection (maybe with a running query) lives until the last `Arc<Session>` drops. In `close`: `KILL QUERY`, lock the editor, `conn.close().await`.
- `Session::info` [design]: needs a real `MySqlSession`, untestable. Make `SessionInfo::new(id, spec, version, db)` pure.
- `SessionInfo` [design]: exposes `identity` but not `account`, so the frontend reimplements `ConnectionSpec::account()` as `accountOf` (`src/api/index.ts`, used in `store.ts`). Add `account` to `SessionInfo`; delete `accountOf`.
- [test]: none. `get` unknown id → `session_not_found`; `remove` twice → `None`; `insert`/`get` same `Arc`. Needs `Session` decoupled from the concrete driver (see driver trait in Part 2).

### connection.rs
- `ConnectionSpec::account` [bug]: no escaping; `user="a@b",host="c"` and `user="a",host="b@c"` share one Keychain entry. Percent-encode `@` and `:` in user (or reject).
- `Secret` [security]: not zeroed on drop; `Clone` copies plaintext; `PartialEq` non-constant-time. Use `zeroize::Zeroizing<String>`; `PartialEq` under `#[cfg(test)]`.
- `Secret` [design]: no `Deserialize`, so `ConnectArgs.password` and `IpcMessage::Launch.password` are plain `String`. Add `Deserialize` (and a `serialize_with` for the IPC wire format).
- [test] missing: identity without database (`"wp@127.0.0.1:3306/"`, the key format recents/ui_state use); `display_name` socket+db and without db; `ConnectionSpec` JSON never has a `password` key.

### launch.rs
- `LaunchQueue::push` [design]: handles an impossible `None` from `into_launch()`. Destructure the `Launch` arm directly.
- `passwords` [security]: one-shot passwords never expire. Store an `Instant`; drop entries older than ~5 min on every `push`/`take_password`.
- [test] missing: `Eval` → no event, returns `false`; connect without password → `hasPassword:false`, `take_password` `None`; unique `request_id`s; `save_password` carried; `push` returns `true` for Connect.

### cli.rs
- `parse_url` [bug]: IPv6 host keeps brackets (`"[::1]"`), sqlx can't resolve. Use `url.host()` and format `Host::Ipv6`.
- `parse_url` [bug]: `?socket=` empty → `Some("")`. Apply `non_empty`.
- `parse_url` [bug]: `mysql://h/db/extra` → database `"db/extra"`. Reject multi-segment paths.
- `build_launch` [bug]: `wants_connection` ignores `--port`, `--name` and the password flags; `databee -P 3307` silently activates only. Count them or fail with "no connection target given".
- `read_password_line` [bug]: EOF → `Some(Secret(""))`. Return an error when nothing was read.
- `CliLogLevel` [design]: duplicates `LogLevel`. Derive `clap::ValueEnum` on `store::settings::LogLevel`; delete the copy.
- `Args::url` help [security]: URL password is visible in `ps` for the whole session when this process becomes the app. Warn in help; fix the `ipc.rs` module doc that claims otherwise.
- [test] missing: IPv6, `mariadb://`, invalid percent-encoding error, `mysql://@h` (empty user → root), empty stdin, CRLF, `--name`, `--save-password`, port-only, empty `?socket=`, `-H` overriding URL host, `--password-env` beating a URL password; `password_sources_are_mutually_exclusive` checks one of three pairs.

### ipc.rs
- `try_forward` [bug]: `Ok(false)` for both "no instance" and "not acked". Return `enum Forward { Delivered, NoInstance, Rejected }`.
- `bind` [bug]: unconditionally `remove_file`s a socket that may be live. Connect first; unlink only on `ConnectionRefused`.
- `bind` [security]: chmod 0600 after bind leaves a window at umask perms; config dir is 0755. Create the config dir 0700.
- `serve` [security]: `read_line` has no size cap or timeout. `.take(64 KiB)` and `timeout(5 s)`.
- `serve` [bug]: accept error → tight `continue` loop. Sleep ~100 ms.
- `serve` [security]: no peer check. Require `peer_cred().uid == getuid()`.
- `serve` [bug]: `let _ = write_all(ACK)`. Log at debug.
- `IpcMessage` [security]: derives `Debug` with plaintext `password`. Manual `Debug` or carry `Secret`.
- module doc [comment]: "passwords never appear in argv of the running app" is false for the first instance.
- [test] missing: stale socket file with no listener; malformed line not acked; `Error`/`Activate` round trips; two clients at once; oversized line.

### commands.rs
- `update_settings` [design]: side effects (Stage restart, theme, log level) in a command; `start_app` repeats theme and log level. Extract `apply_settings(app, previous, saved)` used by both. (Stage restart moves into the Stage plugin, Part 2.)
- `update_settings` [bug]: `log::set_max_level(saved.log_level)` drops the CLI `--log-level` override on the first settings change. Keep the override in state.
- `update_settings` [bug]: `get()` then `update()` not atomic; have `SettingsStore::update` return `(previous, saved)` under the lock.
- `apply_window_theme` [design]: window helper in the command module, swallows `set_theme` errors. Move to `window.rs`, log failures.
- `log_frontend_error` [security]: unbounded, newline-containing string into the log (log forging). Truncate to 4 KiB, escape newlines.
- `forget_password` vs `list_favorites` [design]: one takes a spec, the other an account string. Pick one convention (the spec; it kills `accountOf`).
- [test] missing: `mailto:`, uppercase `HTTPS://`, URLs with credentials; `apply_settings` once extracted.

### db_commands.rs
- `connect` [design]: resolves the password, consumes the launch password, remaps auth errors by source, saves to the Keychain, updates recents. Service logic. Move to `session::open(state, args) -> AppResult<Arc<Session>>`; make `auth_error(source, account, message) -> AppError` pure and tested.
- `connect` [bug]: `take_password(id)` consumes the one-shot password before the attempt; a timeout loses it. Remove only on success (or put it back on a non-auth failure).
- `connect` [bug]: with both `password` and `request_id`, `or_else` short-circuits and the launch password leaks. Always take it when `request_id` is present.
- `connect` [perf]: `secrets.get/set` (Security framework, may block on a dialog) and `recents.touch` (file write) run synchronously on a tokio worker. `spawn_blocking`.
- `ConnectArgs` [security]: `Debug` over `password: Option<String>`. Use `Option<Secret>`.
- `execute` [security]: `log::info!("Query failed: {e}")` logs MySQL error text that quotes SQL at Info. Log `kind` and `code` at info, message at debug.
- `execute` [perf]: `history.record` rewrites up to 500 SQL texts synchronously per query. Cap stored SQL (64 KiB), write via `spawn_blocking` or a debounced writer.
- `execute` [design]: elapsed measured twice (here and in the session). Use `r.elapsed_ms`.
- `cancel_query` [bug]: always `Ok`, even when `KILL` failed. Make `kill` return `AppResult<()>`.
- `disconnect` [bug]: doesn't clear the lens cache nor close the editor connection.
- timeouts [design]: `Duration::from_secs(settings.X_timeout_secs.into())` repeated 3× here and again in `wp_commands`. Add `Settings::query_timeout()` / `connect_timeout()`.
- [test]: none. Testable once logic leaves the commands: the confirmation gate before any session lookup; the `auth_error` table; a failed query still recorded in history.

### error.rs
- `From<sqlx::Error>` [bug]: `Protocol`, `WorkerCrashed`, codes 2006, 2013, 1053, 1927, 4031 fall through to `Database`, so `execute` never reconnects after a server-side kill or `wait_timeout`. Map to `Connection`.
- `AuthFailed` Display [bug]: "Access denied: Access denied for user…". Use `"{0}"`.
- `AppError` doc [comment]: `code?` is always present (null). Write `code: string | null`.
- `Timeout(u32)` [design]: sub-second timeouts show "0 s". Store a `Duration`.
- [test] missing: `PoolTimedOut`/`PoolClosed` → `connection`; `std::io::Error` → `io`; SQLSTATE fallback; 1045 → `auth_failed`, 1317 → `cancelled` via a fake `DatabaseError`.

### secrets.rs
- `MemoryStore` [design]: test-only but in the release binary. Gate with `#[cfg(any(test, feature = "test-util"))]`.
- `resolve_password` [design]: a Keychain failure becomes `PasswordSource::None` with no hint to the UI. Add `PasswordSource::Unavailable(reason)`.
- `PasswordSource` doc [comment]: describes `db_commands` caller behavior. Move to the auth-error mapping.
- [test] missing: failing `SecretStore` fake gives `None` without panic; `delete` of a missing entry is `Ok`; `secret()` per variant.

### dev.rs
- module doc [comment]: "Compiled out of release builds" is false (runtime `cfg!`). Use `#[cfg(debug_assertions)]` on the module.
- `parse_frame` [bug]: accepts `NaN`/`inf`. Check `is_finite()`.
- [test] missing: NaN/inf, extra whitespace, negative width.

### menu.rs
- `LABELS` / `MENU_DEFAULTS` [design]: two parallel tables keyed by the same ids in different modules. Merge into one `MENU_ITEMS: &[(id, label, default_binding)]`.
- `build` [bug]: `LABELS.iter().find(..).unwrap()` panics at startup (and `panic = "abort"` kills the app). Add a test that every id in `build` and `MENU_DEFAULTS` has a label.
- `build` [bug]: a user binding from `keybindings.json` is passed to `accelerator` unvalidated; a bad entry may abort startup via `?` in setup. Validate in `KeybindingsStore::update`; in `build`, skip and log bindings that don't parse.
- `forward` [bug]: `let _ = app.emit(..)`. Log it.
- `set_keybindings` [design]: the menu is never rebuilt; rebuild on `set_keybindings`.
- [test]: none. Testable: table consistency; `is_menu_command(id)`.

### db/mod.rs
- [design]: DTOs plus SQL building in one file. Split into `db/types.rs`, `db/browse.rs`, and `db/dialect.rs` (`quote_ident`, `sql_string`, later per-engine).
- `browse_sql` [bug]: raw filter before `ORDER BY`/`LIMIT`; a filter ending in `# note` or `-- x` comments out the `LIMIT` and streams the whole table; unterminated `/*` breaks the statement. Emit `WHERE ({filter}\n)`; reject filters where the lexer reports an unterminated quote/comment.
- `applied_filter` / `browse` [perf]: filter parsed and lexed twice per browse. `browse_sql` returns `(String, Option<AppliedFilter>)`.
- `applied_filter` [bug]: a comment-only filter yields "must be a single condition". Treat as blank.
- `AppliedFilter.kind` [design]: `String` `"ids"|"sql"`. Use `enum FilterKind`.
- `BrowsePage.filter` [design]: `#[serde(default)]` on a serialize-only struct. Remove.
- `BrowseRequest.limit` [bug]: `limit: 0` → `LIMIT 1`, `has_more` always true, no rows. Clamp `>= 1`.
- [test] missing: filter ending in `#`/`--`; `order_by` with a backtick; comment-only filter; `limit: u32::MAX`; `primary_key` with a backtick. Split `id_shorthand_uses_the_primary_key_only_when_there_is_one` (three behaviors).

### db/mysql.rs
- `MySqlSession` [design]: owns lifecycle, editor lock, reconnect, kill, script execution, introspection, browse, apply, row decoding. Split: `db/mysql/editor.rs` (`EditorConnection`: mutex, thread id, reconnect, kill, timeouts), `db/mysql/introspect.rs` (databases, tables, structure, completion), `db/mysql/script.rs` (`run_script`, `decode_row`, column recovery), `db/mysql/mod.rs` (`MySqlSession` facade implementing the driver trait).
- `execute` [bug]: the post-script `SELECT DATABASE()` is outside the timeout and its `?` discards committed results. Run inside the timeout; on failure keep results and log.
- `execute` [bug]: `let _ = self.reconnect_editor(..)` swallows failure; reconnect has no timeout and holds the editor lock. Keep `connect_timeout` in the session, wrap, log.
- `execute` timeout branch [design]: reconnect silently drops temp tables, `@vars`, open transactions. Return a `session_reset: true` flag (or distinct error kind) so the UI can say so.
- `execute` / `run_script` [design]: an error mid-script drops earlier results and doesn't say which statement failed. Return partial results plus the failing index.
- `use_database` [bug]: no timeout; blocks the editor lock forever on a half-dead connection. Wrap in `query_timeout`.
- `databases`, `tables`, `structure`, `completion` [bug]: no timeouts. Apply `query_timeout`.
- `kill` / `cancel` [bug]: needs a pool connection; if all 3 are busy, cancel waits up to `acquire_timeout` or fails. Use a one-off connection for `KILL`.
- `browse` / `fetch` timeout [perf]: `close_on_drop()` drops the socket but the server keeps running the query. Read `CONNECTION_ID()` and `KILL QUERY` on timeout.
- `apply` timeout [bug]: the pooled connection returns to the pool mid-transaction (protocol desync). `close_on_drop()` on timeout; a timeout after `COMMIT` was sent should say the outcome is unknown.
- `run_script` column recovery [bug]: `returns_rows` is true for `WITH … DELETE/UPDATE`; the PREPARE branch forces `affected_rows = 0`. Only recover when `affected_rows == 0` and the statement after the CTE is a SELECT.
- column recovery [perf]: `conn.prepare` leaves a server-side prepared statement per distinct empty SELECT on the editor connection. `statement_cache_capacity(0)` on the editor options, or close the statement.
- column recovery [bug]: `if let Ok(stmt) = conn.prepare(..)` swallows errors. Debug log.
- `decode_row` / `cell::decode` [perf]: `family(type_name)` runs per cell. Compute `Vec<Family>` once per result set.
- `columns_of` + PREPARE mapping [design]: two identical `ColumnMeta` closures. Extract `column_meta(&MySqlColumn)`.
- `tables` / `structure` [design]: rows read by position (`r[0]`…`r[6]`). Use `FromRow` structs or `try_get("COLUMN_NAME")`.
- `structure` [perf]: three sequential round trips. `tokio::try_join!`.
- `structure` [bug]: `.unwrap_or_default()` hides a missing `SHOW CREATE` row. Return `Option<String>`.
- `connect` [perf]: pool and editor connect sequentially, each with the full timeout. `try_join!` under one timeout.
- `with_timeout` [design]: name hides that it maps to `AppError::Connection`; four other hand-written timeout blocks. `connect_within` plus one `query_within(timeout, conn, fut)` that closes the connection on timeout.
- `raw_text` [design]: DDL via `from_utf8_lossy` turns binary default literals into U+FFFD. Note the limitation or hex-escape.
- `connect_options` [security]: without TLS, `mysql-rsa` fetches the server RSA key in plaintext (MITM can capture the password). Document, or refuse RSA retrieval for non-loopback hosts.
- [test]: none. Testable without a server: `connect_options` (socket vs host, password, database, `timezone(None)`); `run_script` result assembly as a pure fold over `Either<Done, Row>` events (truncation count, `last_insert_id` 0 → `None`, one `ResultSet` per done, trailing push); the column-recovery decision `(statement, result) -> bool` including `WITH … DELETE`.

### db/lexer.rs
- `scan` [bug]: `/*! … */` executable comments are skipped; `/*!40101 SET NAMES utf8 */;` becomes an empty fragment (statement count mismatch) and `/*!50000 DROP TABLE t */` bypasses the safety check. Treat the body of `/*!` as code.
- `scan` word rule [bug]: `$`, non-ASCII bytes end a word; `@` is `Other`. `UPDATE t SET a = @where` counts as scoped. Include `$`, `0x80..` and digits in identifier chars; treat `@name` as one non-keyword piece.
- `split_statements` / `words` [perf]: each call allocates a `Vec<Piece>`; fragments are re-scanned; `destructive_reason` and `returns_rows` scan again and uppercase every word. Make `scan` an iterator; safety reads the first word and searches `WHERE` with `eq_ignore_ascii_case`.
- module doc [comment]: should name known limits (`NO_BACKSLASH_ESCAPES`, `ANSI_QUOTES`, `DELIMITER`) because `run_script`'s count heuristic depends on them.
- [test] missing: `""` → `[]`; unterminated quote/comment; `/*!`; backslash inside backticks; `--` at end; CRLF; multibyte next to quotes; `$`/`@` identifiers.

### db/safety.rs
- `check_statement` [bug]: `has_where` matches a `WHERE` inside a subquery (`UPDATE t SET a = (SELECT x FROM y WHERE z)`). Track parenthesis depth; count only depth 0.
- `check_statement` [bug]: MariaDB `CREATE OR REPLACE TABLE` and `RENAME TABLE` aren't flagged.
- `returns_rows` [design]: a classifier, not a safety rule. Move to `lexer.rs` or `classify.rs`; also misses `CALL`, `CHECK`, `ANALYZE`, `OPTIMIZE`, `REPAIR`, `CHECKSUM`, `HANDLER`, leading `(SELECT`.
- [test] missing: `WHERE` in subquery; `@where`; `/*!`; `WITH … DELETE`; `DELETE … LIMIT 1` without `WHERE`; `CREATE OR REPLACE`; the reason text for `DROP`.

### db/cell.rs
- `bytes` [perf]: `format!("{b:02x}")` per byte. Write into `String::with_capacity(32)`.
- `decode` Text [perf]: full `LONGTEXT` crosses IPC per row; `max_cell_chars` is applied only in `ResultsGrid.tsx`. Truncate in Rust to `{len, preview}` like `Bytes`, with an explicit fetch for the full value.
- `decode` [design]: keyed on a type-name string per cell. `decode(family, raw)` with a precomputed `Family`.
- [test] missing: invalid UTF-8 `VARCHAR` → `Bytes`; control chars in `VARBINARY` → `Bytes` while `\t\n\r` allowed; empty binary; `TIME '838:59:59'` and negative; `YEAR`, `BOOLEAN`; `GEOMETRY`; unknown type → `Text`.

### db/id_filter.rs
- `parse_ids` [bug]: `"+5"` parses (leading `+`). Require a leading digit.
- `to_sql` [design]: walks `parts` twice. `Iterator::partition`.
- `Part`, `IdFilter.parts` [design]: public but module-internal. `pub(super)`.
- [test] missing: `u64` overflow; `"+5"`; `"1-2-3"`; `"007"`; duplicates.

### store/
- all stores [design]: seven copies of `JsonFile` + `Mutex<T>` + open/load/mutate/save. Extract generic `JsonStore<T>` with `read(|t| ..)` and `update(|t| ..) -> AppResult<R>` that mutates a clone, saves, then swaps.
- recents, favorites, history, snapshots, ui_state [bug]: mutate memory first, save second; a failed save leaves a phantom change until restart. Save-then-commit everywhere (free with `JsonStore`).
- all stores [perf]: `std::sync::Mutex` held during synchronous serialize+write from async commands. `spawn_blocking` or a background writer with coalescing.
- `json_file.rs :: load` [bug]: read errors other than NotFound (permission denied, invalid UTF-8) return the default without quarantining; next `save` overwrites the user's file. Treat every read error like corrupt JSON.
- `load` [bug]: `let _ = fs::rename(..)` swallows a failed quarantine; each corruption replaces the previous `.corrupt`. Log; timestamped backup name.
- `save` [bug]: no `sync_all` before rename. Open, write, `sync_all`, rename.
- `save` [security]: files 0644 under a 0755 dir; `settings.json` holds the Stage token, `history.json` holds SQL. Mode 0600 files, 0700 dir.
- `JsonFile::path` [design]: unused. Delete.
- `json_file` [test] missing: non-UTF-8 file quarantined; unreadable file; second corruption keeps the first backup; no stray `.tmp`.
- `settings.rs :: update` [bug]: saves outside the lock. Hold one lock across save and set.
- `Settings::sanitized` [bug]: an empty `stage.token` from the frontend locks Stage out. (Moves to the Stage plugin's own store; regenerate when empty.)
- `settings.rs` [test] missing: `default_port: 0`; timeout clamps; an unknown enum value (`"theme":"neon"`) currently quarantines the whole file and loses every setting. Consider per-field leniency (`#[serde(deserialize_with)]` falling back to default).
- `keybindings.rs :: update` [bug]: no validation of ids or syntax. Add `sanitize`.
- `accelerator` [bug]: splitting on `+` makes a binding for the `+` key impossible (`"Mod++"`). `rsplit_once('+')` or encode as `Plus`.
- `MENU_DEFAULTS` [design]: claims to mirror `src/commands` with no check. Generate one side from the other, or a test reading the TS defaults.
- `keybindings` [test]: `overrides_persist_and_fall_back_to_defaults` asserts five behaviors; split. Missing `"-"` → `Minus`, `"Mod++"`.
- `favorites.rs :: Favorite.id` [design]: "Empty when creating; assigned on save." Accept `NewFavorite` without id in `save`.
- `FavoritesStore::save` [bug]: an unknown non-empty id is treated as new and silently re-id'd; updating skips the duplicate-target check. Return `InvalidInput` for unknown ids; dedupe on update.
- `favorites` [test] missing: unknown id; table target; reorder with unknown ids; same target in different scopes stays two.
- `history.rs :: record` [perf]: `retain` comparing full SQL across 500 entries plus a full rewrite per run. Cap SQL size; compare a hash.
- `history` [test] missing: same SQL on different accounts kept twice; error field stored; the 500 cap is global across accounts (undocumented).
- `snapshots.rs` [perf]: every snapshot's full `values` map is loaded at startup and rewritten on each change; `list` needs only metadata. One file per snapshot plus an index.
- `MAX_PER_SERVER` [design]: cap per account across databases while `list` filters by account+database; one database's snapshots evict another's. Cap per `(account, database)`.
- `snapshots` [test] missing: `get` unknown → `invalid_input`; cap evicts across databases; other accounts untouched.
- `ui_state.rs` fields [comment]: `/// WordPress detail pane, docked right.` etc. stand in for names. Rename (`detail_pane_width`, `inspector_width_px`), drop comments.
- `UiStateStore` [design]: never pruned when a recent is removed. Prune in `remove_recent`, or cap.

### tests/mysql.rs, tests/php_roundtrip.rs
- helpers [design]: `servers`, `spec`, `sessions` duplicated across `mysql.rs`, `wp.rs`, `php_roundtrip.rs`. Move to `tests/common/mod.rs`.
- `cancel_interrupts_a_running_query` [test]: fixed 500 ms sleep; `SLEEP(10)` equals the 10 s `TIMEOUT`. Poll `PROCESSLIST`; `SLEEP(30)` with a longer timeout.
- `timeout_aborts_and_session_recovers` [test]: never runs `USE mysql` first, so database restore on reconnect isn't exercised.
- missing integration cases [test]: editor killed from outside → `connection` error then recovery; error mid-script; `WITH … DELETE` keeps `affected_rows`; `CALL` with two result sets; `SELECT … INTO @x`; `use_database("missing")`; browse with id shorthand and with a `#` filter; browse timeout; `structure` on a view; `fetch` truncation at 50k; `apply` rollback on second-statement failure; `close()` kills a running query.
- `php_roundtrip.rs` [design]: URL-to-spec parsing copied in both tests, no percent-decoding. Reuse `cli::parse`.
- `every_serialized_value_round_trips` [test]: skips `Bytes` cells (the non-UTF-8 values where fidelity matters most); ignores `rs.truncated` at 50k rows; interpolates `{database}`/`{column}` with raw backticks (use `quote_ident`). Fetch `HEX(col)`, page with LIMIT/OFFSET, report skipped counts.
- `wordpress_mode_queries_work_on_real_data` [test]: silently passes when `DATABEE_WP_TEST` is unset; asserts only one search hit. Panic when unset under `--ignored`; assert non-empty lists.

### Cargo.toml, tauri.conf.json
- `tauri-plugin-single-instance` [design]: unused. Remove.
- clap `env` feature [design]: unused. Remove.
- `[lib] crate-type` [perf]: `staticlib`/`cdylib` only for mobile; slow desktop links. `["rlib"]`; delete the two template comments.
- `panic = "abort"` [design]: a panic in any command or task kills the process with the user's open transactions. Keep `unwind`, and audit runtime-reachable `unwrap`/`expect`.
- `app.security.csp: null` [security]: no CSP while the webview renders arbitrary DB content. Set `default-src 'self'; img-src 'self' data: https://www.gravatar.com; style-src 'self' 'unsafe-inline'; connect-src ipc: http://ipc.localhost`.
- `bundle.icon` [design]: lists `icon.ico` for a macOS-only bundle. Drop.

### Rust core, top 10 by impact
1. Single-instance handoff (`run`, `try_forward`, `bind`): timeouts, missing acks and simultaneous launches lead to two instances and an unlinked live socket.
2. `browse_sql`: a filter ending in `#`/`--` comments out `LIMIT`.
3. `execute`: post-query `SELECT DATABASE()` outside the timeout discards committed results; reconnect swallowed and unbounded.
4. sqlx error mapping: disconnects map to `Database`, so the editor never reconnects.
5. `apply` timeout returns a mid-transaction connection to the pool.
6. Safety bypasses: `WHERE` in subqueries, `@where`, `$`, `/*! … */`.
7. Column recovery: `WITH … DELETE` loses `affected_rows`; prepared statements pile up.
8. File and socket security: 0755/0644, chmod race, unbounded `read_line`, `csp: null`.
9. Secrets in `Debug`; one-shot passwords never expire; URL password in `ps`.
10. Blocking I/O and the store pattern: Keychain and full JSON rewrites on tokio workers; memory mutated before save; unreadable files clobbered.

## 2. Stage hook (reviewed directly)

### src-tauri/src/stage/hooks.rs (1405 lines: ~670 code + ~730 tests)
- [design] `Hub` mixes four jobs: request/reply correlation (`Waiter`, `ask`, `take`, `receive_*`), window-model JSON (`windows_json`, the `sheet` closure, `is_shown`, `anchor_json`, `screen`), verb handlers (`window`, `command`, `on_anchor`, `wait`, `keys`, `act`, `describe`) and a 240-line `FakePage` test host. Split: `stage/hub.rs` (correlation + `StageHost` + `Timeouts`), `stage/windows.rs` (`Frame`, window JSON, `is_shown`, anchor JSON), `stage/verbs.rs` (`run`, `describe`, `window`, `act` and the act kinds), `stage/test_host.rs` (`#[cfg(test)]` `FakePage`).
- [design] `window()` is ~85 lines, a nested `match` on `(id, action)` with three separate `self.info()` calls. Parse once into `WindowRequest { window: Window, action: WindowAction, bounds, section }`, then dispatch to `main_window_action` / `sheet_action`.
- [design] stringly typed window ids (`"settings"`, `"palette"`, `MAIN`) compared in `is_shown`, `windows_json`, `window()`, `keys()`, `AnchorsReply::cleaned`, plus `WINDOWS` and `window_order`. Replace with `enum Window { Main, Settings, Palette }` with `FromStr`, `as_str`, `Serialize`/`Deserialize`. Same for act kinds (`"click" | "type" | "reveal" | "scroll" | "wait" | "keys" | "command"`) → `enum ActKind`, parsed once in `act`.
- [design] `on_anchor` validates `type` and `scroll` params inline. Extract `type_payload(params)` and `scroll_payload(params)` returning `StageResult<Value>` (pure, unit-tested).
- [design] `describe`, `windows_json`, `anchor_result`, `wait` build untyped `Map<String, Value>` with `json!`. Use `Serialize` structs (`Describe`, `WindowInfo`, `AnchorInfo`, `ActResult`) with `skip_serializing_if`; tests compare `serde_json::to_value`.
- [comment] module docs cite "protocol 1, revision 2026-10-16, §5", "§5.5", "§5.1", "§3.1", "§9", "§7", "§10" of a spec that is not in the repository. Replace every section reference with a plain statement of the rule or a link to `docs/stage.md`.
- [design] `last_request: Mutex<Option<DateTime<Utc>>>` is fine. `rect_json` uses `unwrap_or(Value::Null)` on an infallible serialization; derive `Serialize` on the output structs instead.
- [test] good behavior coverage. Missing: a reply arriving after the timeout is ignored (waiter removed); `describe` with no window (`frame()` → `None`); two concurrent `ask`s don't cross replies.

### src-tauri/src/stage/http.rs
- [bug] `serve`: on an accept error (EMFILE) the loop spins hot. Sleep ~100 ms after an error.
- [design] `response()` returns `(u16, Vec<u8>)`; the status is unused by `handle` (`let (_, raw)`). Return only the bytes; tests parse the status line.
- [design] `check`'s `Host` parsing (`rsplit_once(':')` with a numeric check) refuses `[::1]:port`. Acceptable (documented loopback only) but add a test and a one-line doc.
- [test] missing: non-numeric `Content-Length` → 400; `READ_TIMEOUT` path; `Host: localhost:abc`.

### src-tauri/src/stage/mod.rs, stage_commands.rs
- [design] `stage_new_token` carries logic (generate, update settings, restart). Belongs in `Stage::rotate_token`.
- [design] `Server` duplicates `enabled/port/token` from settings. After the plugin owns `StageSettings`, keep one copy.
- [design] `TauriHost::frame` returns `None` when any Tauri call fails, which the hub reports as "No DataBee window has keyboard focus". Log the failing call once.

### src-tauri/src/stage/protocol.rs
- [design] `parse_keys` is 60 lines with four exits; split into `split_modifiers(text) -> (StageKeys, &str)` and `parse_last(last, shift) -> Option<(key, code)>`.
- [design] `short(text)` cuts labels at 200 chars server-side while the page cuts at 80 (`MAX_LABEL` in `hooks.ts`). One limit, in one place (the page).

### src/stage/hooks.ts (398 lines)
- [design] mixes anchor collection (`labelOf`, `roleOf`, `detailsOf`, `clipsOf`, `collectAnchors`, `findAnchor`, ~120 lines), DOM actions (`reveal`, `click`, `typeInto`, `scroll`, `press`, `sheet`, ~170 lines) and dispatch (`perform`, `installStageHooks`). Split into `src/stage/anchors.ts`, `src/stage/actions.ts`, `src/stage/hooks.ts` (dispatch only), with tests per file.
- [design] `perform` is 50 lines with a `switch` that falls through into anchor handling; `StageActRequest` is one bag of optionals for every kind (`anchor?, text?, mode?, keys?, command?, window?, action?, section?, by?, to?, focused?`). Make it a discriminated union on `kind` in the stage types so each branch is type-checked, and mirror it with `#[serde(tag = "kind")]` in Rust.
- [design] `nextFrame` uses a comma expression `(clearTimeout(timer), resolve())`; `FAMILIES[0].run` uses `(openDialog(..), true)`. Rewrite as statements.
- [design] `commands.ts` and `state.ts` import `SETTINGS_PAGES` from a component (`components/SettingsDialog`). The settings-page registry (Part 2, F1) removes this.
- [design] `installStageHooks` is called from `App.tsx`; `api/index.ts`, `api/types.ts`, `test/apiMock.ts`, `test/settings.ts`, `store.ts` (`newStageToken`) and `SettingsDialog.tsx` (`PresentationsPage`) all carry Stage code. See Part 2, S1–S3.
- [test] `hooks.test.tsx` is good. Missing: `reveal` calls `scrollIntoView` only when out of view and uses `block: "start"` for tall anchors; `clipsOf` clips by an `overflow: auto` parent; `labelOf` truncation; `wp:open:` family; `stageView` for wp routes; `stageState` for table and query tabs.

### Stage coupling surface today
Outside `src-tauri/src/stage/` and `src/stage/`: `lib.rs` (mod, setup, 5 registrations), `commands.rs` (restart), `store/settings.rs` (`StageSettings`), `stage_commands.rs`; `App.tsx`, `api/index.ts`, `api/types.ts`, `test/apiMock.ts`, `test/settings.ts`, `state/store.ts` (`newStageToken`), `SettingsDialog.tsx` (`PresentationsPage`, `SETTINGS_PAGES` export). The `data-stage*` attributes in 18 components are inert and may stay.

## 3. WordPress layer (`src-tauri/src/wp/`, `wp_commands.rs`, `tests/wp.rs`, `dev/fixture.sql`)

### wp/mod.rs
- [design] five jobs: detection I/O, document loading, link resolution, featured images, two options queries. Move `detect` → `context/probe.rs`; `load_docs`/`expand`/`titles`/`resolve_links` → `entity/load.rs`; `post_images` → `media.rs`; `options_overview`/`options_group` → `options.rs`.
- `load_docs` [bug] (verified): relation counts run for every relation without `rel.when.applies(ctx)`. On a legacy store `user.orders` (`when = "hpos"`) resolves to the posts-based order definition and runs `SELECT customer_id … FROM wp_posts` → "unknown column"; no user document opens. Filter by `when` as `assemble` does.
- `load_docs` [perf]: `sort_by_key(|d| ids.iter().position(|id| id.to_string() == d.id))` is O(n²) with an allocation per compare. `HashMap<u64, usize>`.
- `load_docs` / `resolve_links` [design]: `def.table == "posts"` hard-codes "has featured image" twice. Entity key `image = "_thumbnail_id"`.
- `resolve_links` [perf]: one `titles` plus two `post_images` queries per link target, sequential (~7 queries before view work). Batch titles across entities; skip images for links the UI doesn't render.
- `load_docs` doc [comment]: "meta and relation counts fetched in one query each" is false (one per relation plus links and images). Fix here and in `docs/wordpress.md`.
- `sql_string` [bug] (verified): doubles backslashes while the session keeps the server's `sql_mode`; under `NO_BACKSLASH_ESCAPES` every written `\` is doubled (JSON `\/`, `\"`; LIKE escapes break). Read `@@sql_mode` once per session and escape accordingly, or use bound parameters for values.
- `text` [design] (verified): `mod.rs::text` and `lists::text` are duplicates (and `text("")` → `None`, see changes.rs). One `rows.rs` with `cell_str`/`text`/`num`/`int`.
- `options_group` [bug]: `"transients"` and `"(single)"` are magic strings in the prefix namespace. `enum OptionGroup { Prefix(String), Transients, Unprefixed }`.
- `options_group` [bug]: `LIKE 'p%'` matches other groups (`wc` → `wcpay_*`). `option_prefix` splits at the first `_`/`-`, so filter `LIKE 'p\_%' OR LIKE 'p-%'`.
- `options_group` [bug]: `(single)` filter `NOT LIKE '%\_%'` drops `_foo`, which `option_prefix` classifies as single. Build the filter from the classifier.
- `options_group` [product]: `LIMIT 500` truncates silently. Return `total` and `truncated`.
- `options_group` [design]: autoload state encoded in `DocField.label` (`"name · autoload"`) and parsed back in `Settings.tsx`. Add `autoload: Option<String>` (raw value).
- `options_group` `esc` [design]: third copy of LIKE escaping (`lists::like`, `peek::escaped_like`, `detail::search`). One `sql::like_contains` / `like_prefix`.
- `WpInfo.entities` [design]: `Vec<(String, String)>` "(name, label)" → struct.
- [test]: none. After the split: detect options SQL; `options_group` filter builder; doc ordering; `resolve_links` id collection when pure.

### wp/context.rs
- `site_meta` [bug]: only `gmt_offset` is probed; sites with `timezone_string` (most) get 0 and no DST. Probe `timezone_string`; convert per timestamp with `chrono-tz`.
- `SiteMeta.gmt_offset_secs` doc [comment]: documents the bug as intended.
- `site_meta` uploads_url [product]: `siteurl + /wp-content/uploads` is a guess (ignores `upload_path`, `WP_CONTENT_URL`, multisite `sites/N`). Name the source or omit.
- `site_meta` active_plugins [bug]: network-activated plugins (`active_sitewide_plugins`) ignored on multisite.
- `build` [design]: `has` allocates per lookup; `main_prefix` rescans. Build the `BTreeSet` once.
- [test] missing: sync on + HPOS `no` → `Off`; empty `gmt_offset`; non-serialized `active_plugins`; `upload_url_path` precedence; equal-length prefixes; sub-site without `_options`; WooCommerce detected by `woocommerce_order_items` only.

### wp/php.rs
- `Int` / `Bool` [bug]: `i:+5;`, `i:05;`, `i:-0;` parse but serialize canonical, breaking `serialize(parse(x)) == x`. Reject non-canonical in strict mode (or keep raw text as `Float` does).
- `Float` [bug]: `d:` accepts any bytes to `;`. Validate against PHP's grammar (digits, `INF`, `-INF`, `NAN`, exponent).
- `quoted` lenient [bug]: ends at the first `";` or `":`; nested serialized strings contain `";`, so lenient can succeed with a wrong tree. Verify the whole input is consumed with consistent structure, or recover only near the declared length.
- `quoted` lenient [bug]: when the declared length lands on an inner `"`, the strict fast path accepts a misread.
- `looks_serialized` [bug]: looser than WordPress `is_serialized` (`a: note;` reports "php" with an error). Mirror WordPress's checks.
- lookups [design]: five near-identical key lookups (`detail::php_entry`/`php_str`, `media::entry`, `lists::session_summary` closures, `hints::roles`). Add `PhpValue::get(key)` and `as_text()`.
- `quoted` doc [comment]: em dash.
- [test] missing: `i:+1;`/`i:05;` round trip; `d:-INF;`, `d:NAN;`, `d:1.0E+25;`; private `\0Foo\0bar` round trip; `C:` length mismatch; negative ref; lenient nested false-success; duplicate key preserved; `E:` invalid length.

### wp/value.rs
- `key_of` [product]: private names drop the class (`\0Foo\0baz` → `baz`); protected and public `bar` render identically. Carry `declaring_class` and the raw mangled key.
- `key_of` [bug]: `from_utf8_lossy` on keys makes invalid-UTF-8 keys identical and unaddressable for edits. Carry raw bytes (hex) or an index.
- `ValueNode.kind` / `ChildNode.key_kind` / `DecodedValue.format` [design]: `&'static str` with doc-listed values → serde enums.
- `decode_at_depth` [design]: `DecodedValue` construction copied three times. Constructor.
- [test] missing: depth cap at 4; invalid-UTF-8 note; JSON big ints and `1.0` vs `1`; `custom`/`ref`/`enum` shapes; protected+public same name; `looks_serialized` false positive.

### wp/entity.rs, entities.toml
- `entities.toml user.hidden` [product]: `hidden = ["user_pass", "user_activation_key"]` hides stored data; `meta_keys.toml` already masks them. Remove `hidden` (feature and uses).
- `definitions` [bug]: a user override replaces a built-in only when `name` and `when` both match; a user `order` with default `when` is appended after built-ins and ignored by `Catalog::new` (first match wins). User definitions take precedence by name, or error on ambiguity.
- `assemble` [design]: hard-codes `currency` column and `_order_currency` meta. Entity key `currency = ["currency", "_order_currency"]`.
- `assemble` [design]: ~110 lines, four jobs. Split `bucket_meta`, `doc_hints`, `row_fields`, `relation_infos`.
- `assemble` [perf]: `column_index` per row twice; case-insensitive `hidden` per column per row. Precompute once per `ResultSet`.
- `meta_sql` [bug]: `ORDER BY fk` only; repeated meta keys in arbitrary order → `snapshot::flatten_doc` numbers `[2]`, `[3]` differently → spurious diffs. Add `id` to `MetaDef` (`meta_id`/`umeta_id`/`id`), order by it.
- `MetaDef` [design]: without a row id, `SetMeta`/`DeleteMeta` can't target one row and `peek::SOURCES` repeats every meta table. `id` fixes both.
- `entities.toml action` [design]: no `logs`/`group` relations or links; `detail::action_view` hand-writes both queries. Define as data.
- legacy order / user [design]: user → orders needs a meta foreign key (`_customer_user`). Support `fk_meta` in `RelationDef`, or document the gap.
- `Catalog` [design]: linear `get`/`has` per field. Index map.
- section banners [comment]: `// ---------- SQL ----------`, `// ---------- documents ----------` mark the seams; make them files.
- [test]: `assembles_documents_…` names `total` for the `status` field. Missing: `relation_ids_sql` exact SQL; non-applying `when` excluded; user override with different `when`; `meta_links`; currency from `_order_currency`; case-insensitive id column; empty title → `None`; link to a missing entity.

### wp/hints.rs, meta_keys.toml
- `sniff` / `TIME_WORDS` [product] (verified): hints classified from key-name substrings (`_at`, `last_`, `date`, `user_agent`, `gmt`), the heuristic the rules ban. Mark sniffed hints `guessed: true` for the UI; prefer value-shape sniffing.
- `time` [bug]: fixed `gmt_offset_secs` (no DST, 0 for named zones).
- `money` [bug]: `inf`/`NaN` render `$inf.00`; large decimals lose precision; `-0.001` → `−0.00`; decimals fixed at 2 (ignores `woocommerce_price_num_decimals`). Decimal type, reject non-finite, probe the option.
- `rule_for` / `glob` [perf]: every rule's glob per annotation, each allocating a `Vec` (≈300k allocations for a 50-row list). Precompile patterns at load.
- `rule_for` [design]: returns `KeyRule` with dummy `pattern`. `Resolved { render, sensitive }`.
- `annotate` / `hint_for` [design]: `Render::Text` handled in both.
- [test] missing: rule merge order; `Render::Text` suppressing a sniff; `inf` money; RFC 3339; epoch boundaries 999_999_999 and 4_102_444_801; multi-star globs; lowercase country rejected; `date_*_gmt`; `"0"` flag.
- `meta_keys.toml` [comment]: `# ---------- PayPal Payments ----------` groups keys by assumed owning plugin. Neutral section names.

### wp/lists.rs
- `t` doc [comment]: "`wc-processing` → `Processing`." on a table-reference builder; stale.
- `Page.facets` doc [comment]: "independent of the current filter" false for actions.
- module doc [comment]: "one query per page (plus one batched)" outdated (3 to 6).
- `HIDDEN_POST_TYPES` [product]: revisions, nav items, templates, `custom_css`, `oembed_cache` excluded from the list and counts and reachable nowhere. Collapse under an "internal" facet.
- `products_sql` / `content` [product]: `trash`, `auto-draft` excluded from rows and facets. Exclude by default, keep as facet.
- `guests_sql` [bug]: `{email}` placeholder replaced after user input is inserted; searching the literal `{email}` rewrites the term. Build the column name first.
- `guest_rows` [product]: `roles: vec!["Guest"]` is a made-up label in a slug field; `registered` holds first-order date. Explicit `guest: bool`, `first_order`.
- `users_sql` [bug]: `meta_value LIKE '%"editor"%'` also matches `s:6:"editor";b:0;`. Filter in Rust after `roles_of`, or match `"editor";b:1`.
- `customer_stats_sql` / `guests_sql` [bug]: totals summed across mixed currencies. Group by currency.
- paid statuses [design]: `('wc-completed', 'wc-processing')` copied five times. One `PAID_STATUSES`.
- `orders_sql` legacy [perf]: search/gateway filters in `HAVING` after pivoting all postmeta per keystroke. `EXISTS (SELECT 1 FROM postmeta …)` in `WHERE`.
- `orders_sql` [design]: ~90 lines, two storage branches, string `+=`. Split `orders_sql_hpos` / `orders_sql_legacy` over a `Filter` builder (`and`, `search_any`, `id_or`).
- id-match pattern [design]: `parse::<u64>().map(|n| format!("X = {n} OR "))` ×5 → builder.
- `order_by` [design]: default direction from a key-name blacklist; lists looked up by string (typo → silent fallback). `default_desc` per `SORT_KEYS` entry; `ListKind` enum.
- `order_by` unknown list [bug]: emits `ORDER BY id DESC, id DESC`.
- `products_sql` [design]: rewrites `sort` (`name → title`) to reuse the `content` whitelist; add its own entry.
- `products_sql` [design]: price range built in SQL (`CONCAT(min,'–',max)`) then split in Rust. Select two columns.
- `products_sql` [bug]: duplicate rows for products with >1 `product_type` term; kind facet counts trash while status facet doesn't.
- `products` [product]: `"(no title)"` replaces an empty title. Return `None`.
- `order_rows` [bug]: `filter(|r| r.len() > ORDER_COLS)` drops rows silently. Named constant + `debug_assert`.
- `action_rows` [product]: `args` `"[]"` → `None`.
- `actions_sql` / `failed_by_hook` [bug]: ignore `extended_args`.
- `facets` [design]: `label` closure is always identity; `dashboard.rs` has two more copies. One function, drop `label` or the closure.
- `session_summary` [design]: `(u64, Option<String>, Option<String>)` → `CartSummary`.
- `int` [design]: parses via `f64` then `as i64` (precision loss). `i64` first.
- `failed_by_hook` [comment]: fully qualified `std::collections::HashMap` with the import present.
- [test]: `sql.contains` checks (loose); a `use` between tests. Missing exact SQL: `users_sql` role filter; `guests_sql` both modes; `customer_stats_sql`; `products_sql` ± lookup table; `actions_sql`; legacy HAVING; `like` escaping `\` and `_`; `page_split` at limit, limit+1, 0; `ListQuery::limit` clamping.

### wp/detail.rs (2162 lines)
- module [design]: seven views, shared address/timeline types, PayPal fees, HTML entities, omnisearch. See layout in Part 2.
- `order_view` [design]: ~455 lines. Split `load_addresses`, `order_dates`, `order_totals`, `order_children`, `payment_panel`, `attribution`, `build_timeline`, `related_chips`, pure over the doc.
- `log_level` [bug] (verified): WooCommerce levels 100 debug, 200 info, 300 notice, 400 warning, 500+ error; current `0..=250 info, 251..=300 warning, _ error` shows notice as warning and warning as error. Use `..=300 info, 301..=400 warning, _ error` (or map each level name).
- `order_logs` [bug]: context pattern `%"order": 4711%` assumes JSON; `WC_Log_Handler_DB` stores serialized PHP (`s:5:"order";i:4711;`). Fixture contexts are NULL so tests miss it.
- `order_logs` [bug]: `message LIKE '%#4711%'` matches `#47110`; PayPal/transaction ids inlined unescaped. Word-boundary REGEXP plus the shared LIKE escaper.
- `order_logs` [perf]: `load_docs` for the full order to read three values; `epoch("timestamp") BETWEEN …` in `WHERE` defeats indexes. Fetch the three values; compare against datetime literals.
- scheduled actions [bug]: `args LIKE '%4711%' … LIMIT 200` truncates before the exact `args_reference` filter; ignores `extended_args`. `JSON_CONTAINS` or drop the LIMIT before the Rust filter.
- scheduled actions [perf]: full scan per order view; `snapshot::capture` copies the query. One `actions_referencing(id)`.
- totals [product]: subtotal/discount/shipping/fees/tax recomputed from item meta in `f64`, labelled without source keys; stored totals (`shipping_total`, `discount_total`, `tax_amount`, `_order_shipping`) ignored. Show the stored column/meta per line; flag differences.
- totals [bug]: money summed in `f64`. Decimal type or sum in SQL.
- timeline [product]: notes classified `error` by substrings `fail`/`error`; created/paid/completed/note/action events have no `row` pointer. Set `RowRef`; drop keyword classification.
- timeline [product]: note author `"WooCommerce"` filtered out of `detail`.
- event closure [design]: six positional args with `&'static str` kind/level. `TimelineEvent::new(kind, at, title)` + builder setters; enums.
- `PaymentPanel` [design]: PayPal fields (`_ppcp_*`, `paypal_order_id`, `intent`, `mode`, `payer_email`, "PayPal Transaction Fee") hard-coded. Panel = `(key, value)` rows from a TOML `[payment] keys = […]` table.
- fees [design]: `Fees` construction copied; the copy doesn't clamp `fee_ratio`. `Fees::from_amounts`.
- attribution [product]: `trim_start_matches("utm_")` shows `source` instead of `_wc_order_attribution_utm_source`. Keep the full key.
- customer [perf]: separate `titles` query although `resolve_links` filled `customer_id`'s link; `session_view` same. Reuse `doc.field("customer_id").link`.
- `order_items` [product]: drops `_` keys, empty values, serialized values; item meta appears nowhere. Keep collapsed.
- `order_items` / `decode_entities` [product]: decoded value replaces stored text. Show as label next to raw.
- `order_items` [design]: returns `(Vec<LineItem>, HashMap<String, f64>)` with string keys → `Totals` struct.
- `address_mismatches` [bug]: compares `lines.last()`; with empty city/state/postcode that's `address_1` → false positives. Compare structured fields.
- `Address::is_empty` [bug]: ignores `company`, `country`.
- HPOS addresses [design]: `col(i)` index table copies SELECT order. `column_index` by name.
- `customer_orders` [design]: `replacen("WHERE o.type", …)` on generated SQL. Add `customer: Option<u64>` to `ListQuery` (the frontend already sends `customer=`).
- `customer_view` [bug]: Subscriptions chip count = `subscriptions.len()` capped at 10. Count in SQL.
- `customer_view` [design]: five-tuple → `CustomerStats`.
- `guest_view` [bug]: `lists::orders` with `search = email`, `limit 10`, then exact filter; superstrings fill the limit. Query by exact `billing_email`. Two facet queries wasted.
- `guest_view` [bug]: `session_value LIKE '%email%'` unescaped `_`, scans all sessions.
- `guest_view` [design]: fake `EntityDoc` (`entity "user"`, `id "guest:…"`, `table postmeta`); Live/snapshot/compare call `load_docs("user", "guest:x")` and fail. Own `GuestView` type.
- `product_view` [product]: strips `pa_`/`attribute_`, empty variation values → `"Any"`, taxonomies filtered to cat/tag/brand, trashed variations excluded. Raw keys next to labels; all taxonomies; collapse trash.
- `product_attributes` doc [comment]: "(name, values)" vs a `(String, String, bool)` triple. Struct.
- `product_view` comment [comment]: "in two queries" outdated (four).
- `entity_view("post")` [perf]: loads the doc, then `product_view`/`order_view` load it again. Pass it in.
- `product_variation` [product]: opening a variation renders the parent. Render the variation with a parent link.
- `action_view` [bug]: `pick(["args", "extended_args"])` prefers `args`, which holds a hash for long args. Prefer non-empty `extended_args`.
- `action_view` [perf]: logs unbounded; group slug separate query. Join; cap with count.
- `search` [perf]: ~17 sequential queries per keystroke including exact `meta_value =` on unindexed longtext. Debounce in UI; run concurrently; gate meta scans ≥ 8 chars.
- `search` [design]: ~170 lines; entity remapping (`starts_with("shop_order")`) twice; `"option"` hit kind isn't an entity. `SearchSource` table like `peek::SOURCES`.
- `search` [bug]: under HPOS+sync a numeric search returns `order 4711` and `post 4711`.
- `RelatedChip.target` [design]: stringly routes (`"list:orders:search=…"`, `"timeline"`), parsed with `startsWith` in `OtherViews.tsx`. Serde enum `ChipTarget`.
- section comments [comment]: `// Addresses: HPOS table or legacy meta.`, `// Refunds and subscriptions hanging off this order.`, `// Product names, images and SKUs in one batch each.`, `// Names, titles and SKUs.`, `// ---- timeline ----`, `// ---- relation strip ----` narrate.
- field docs [comment]: `/// created, paid, … refund`, `/// info, warning, error`, `/// Route hint …` → enums.
- [test]: `decode_entities`, `paypal_fees`, `address_mismatches`, `from_parts`, `args_reference`, `token_summary`, `product_attributes` covered. Missing: `log_level`; `paypal_fees` without `net_amount` / gross 0; `decode_entities("&amp;lt;")`; mismatches with no city; variation mapping; totals; timeline ordering; `entity_view` dispatch; search SQL per input shape. The last four need the logic out of async fns.

### wp/dashboard.rs
- `table_category` [bug]: `contains("log")` before the core list; `wp_blogs`, `*_catalog_*`, `login_*` → "logs". Core first; `ends_with("_log"|"_logs")`.
- `table_category` [product]: "plugins"/"woocommerce"/"logs" by table-name heuristics. Group by literal prefix.
- `insights` [product]: "holds 63% of all rows" from `TABLE_ROWS` estimates. Say "≈" or count the top table.
- hour fallback [design]: sentinel `DayBucket { date: "__today" }` pushed and popped; narrating comment. Local `today_total`.
- `dashboard` [bug]: `DATE(date_created_gmt)`/`UTC_DATE()` are UTC days; `Utc::now()` vs server clock. Site timezone; `SELECT UTC_TIMESTAMP()`.
- `day_buckets` / `hour_buckets` [design]: status folding copied; `cancelled` folded into `failed` (unproven). One `fold_status`, a `cancelled` bucket.
- `action_histogram` [design]: reuses order `DayBucket` by renaming AS statuses; `canceled` → `other`. `BTreeMap<status, count>` or own type.
- drift [bug]: `post_modified_gmt <> date_updated_gmt`; NULL never counted; second-level sync differences show as drift. Compare status/total/type; name the columns in the insight.
- `dashboard` [perf]: recent orders via `lists::orders` (two facet queries wasted); `failed_by_hook` fetches 100 keeps 6.
- `dashboard` [design]: ~215 lines. `order_stats`, `action_stats`, `autoload_stats`, `hpos_drift`.
- autoload SQL [design]: `IN ('yes','on','auto','auto-on')` copies `options::is_autoloaded`. One `AUTOLOAD_ON`.
- `browser` [perf]: 7 to 9 sequential COUNTs per sidebar refresh. `try_join!` or `UNION ALL`.
- `InsightAction` / `Insight` / `TableTile` / `BrowserSection` [design]: `&'static str` kinds/levels/categories → enums; `go()`/`change()` returning `Option` odd.
- [test]: `table_category` lacks `wp_blogs`. Missing: hour-mode today; `active_days < 3`; `AUTOLOAD_HEALTHY_BYTES` boundary; ≤ 10,000 rows; non-log top table; histogram status mapping.

### wp/changes.rs
- `SetCell` / `SetMeta` undo [bug] (verified): `first_text` → `lists::text` turns `""` into `None`; undo writes `NULL` for an empty value. Use `cell_str`, keep `Null` and `""` distinct.
- `SetCell` [bug]: binary old value → `None` → undo writes `NULL`. Empty `undo` for `Cell::Bytes`.
- `SetCell` [bug]: when `column == key_column`, the undo `WHERE key_column = old_key` matches nothing. Use the new key.
- `SetMeta` [bug]: with duplicate meta rows all are updated; undo restores all to the first row's value. Plan per `meta_id` (needs `MetaDef.id`) or refuse `n > 1`.
- `SetMeta` / `DeleteMeta` [product]: under `OnWithSync` only `wc_orders_meta` is written; posts copy diverges (the drift the dashboard reports). Mirror or warn.
- `DeleteMeta` [bug]: plans a no-op DELETE when nothing matches. Refuse like `DeleteRow`.
- `SetOrderStatus` [bug]: undo restores `status` but not `date_updated_gmt`, `post_modified(_gmt)`.
- `SetOrderStatus` [bug]: `post_modified = NOW()` is session time; WP stores site-local.
- `RetryAction` [bug]: sets `scheduled_date_gmt` not `_local`; undo misses `claim_id`, `attempts`; cancel doesn't check the table exists.
- `DeleteLogsBefore` [bug]: `UTC_TIMESTAMP()` at apply time; planned count can differ. Cutoff literal at plan time.
- `DeleteExpiredTransients` [perf]: calls `options_overview` (reads every option). Query `_transient_timeout_%` with `option_value < now`; orphan timeouts never cleaned.
- `DuplicateRow` [bug]: no row-count check (non-unique key duplicates all), doesn't exclude generated columns.
- `reinsert_sql` [bug]: `SELECT *` re-insert fails on generated columns; zero dates fail under `NO_ZERO_DATE`. Skip generated columns via `structure()`; warn.
- `reinsert_sql` [comment]: `Cell::Bytes { .. } => "NULL"` arm unreachable.
- `plan` [design]: ~410-line match. One `plan_*` per variant over pre-fetched state (pure, testable); `meta_target(ctx, catalog, entity, id, key)` shared by SetMeta/DeleteMeta.
- `ChangeRequest` [design]: add `fn database(&self)`; `wp_commands::plan_change` has a 12-arm copy.
- `edit_serialized` [bug]: `raw.trim()` discards stored whitespace.
- `edit_serialized` JSON [bug]: `serde_json::to_string` re-escapes and normalizes; "untouched parts stay byte-identical" is false for JSON. Splice the edited span, or restrict the claim to PHP.
- `key_matches` [bug]: paths by display key; demangling in arrays; protected vs public `bar`, int `1` vs `"1"` collide. Address by index.
- `set_php` [bug]: replacing a subtree shifts PHP value numbering; later `r:N`/`R:N` point wrong. Refuse when `Ref` nodes exist at or after the edit.
- `php_new(Float)` [bug]: Rust `f64` accepts `inf`/`nan`/`infinity` → `d:inf;` unreadable by PHP. PHP grammar.
- `SetMeta` [bug]: doesn't verify the entity row exists (orphan meta).
- [test]: `edit_serialized`, `logs_before_filter`, `reinsert_sql` covered; `plan()` has none. Missing: empty-string undo; multi-row meta; `SetAutoload` styles; `SetOrderStatus` `wc-` and `trash`; transient pairs; `reinsert_sql` with Bytes/Temporal/Json; JSON escape preservation; edit next to a `Ref`; Float `"inf"`.

### wp/options.rs
- module doc [comment]: "by plugin prefix" contradicts the rule. "literal name prefix".
- `option_prefix` [bug]: `contains("_transient_timeout_")` vs `starts_with`.
- `cleanup_sql` [design]: re-implements `sql_string`; duplicates `DeleteExpiredTransients`; still reachable via `wp_transient_cleanup_sql` (unused by the UI). Remove.
- `overview` [bug]: orphan timeout rows neither listed nor counted.
- [test] missing: `_foo`; `-` separators; `transients_foo` vs magic group; site-transient cleanup; orphan timeouts; "auto-off" total; tie ordering. `cleanup_sql_targets…` uses `contains`.

### wp/peek.rs
- `peek` [bug]: four user specs + Orders + Spent = six, then `truncate(5)`; "Spent" always dropped. Raise the cap or reorder.
- `peek` [bug]: order stats HPOS-only; copies `customer_stats_sql` with different semantics. Reuse it.
- `fact_from` [bug]: first field in document order whose name is in `names`, not preference order. Iterate `names`.
- `fact_from` [design]: re-implements `EntityDoc::all_fields`; `"status"` kind from `label == "Status"`.
- `PeekFact` [product]: label without source key. Add `key`.
- `FACTS` [design]: per-entity summary fields hard-coded. Move to `entities.toml` `peek = [{ label, keys }]`.
- `SOURCES` [design]: copies `EntityDef.meta` minus the row id. Derive from the catalog once `MetaDef.id` exists; keep only extras static.
- `peek` [perf]: full `load_docs` per hover. Row + meta only.
- `find_value` [perf]: COUNT then rows per source, 10 sources sequential (~20 scans). `LIMIT n+1`, COUNT only when exceeded.
- `find_value` [design]: condition closure written twice.
- `find_value` [product]: `posts`, `users.user_email`, `comments` never searched.
- field docs [comment]: `/// (label, candidate field or meta names)`; `/// text, status, money, time …` → enum.
- [test]: none. `fact_from` over a hand-built doc; user-fact truncation/order; a `find_sql(src, value)` builder; consistency of `FACTS`/`SOURCES` names with `entities.toml`.

### wp/snapshot.rs
- `flatten_node` [bug]: paths joined with `/` unescaped; keys containing `/` collide in the `BTreeMap`. JSON-pointer escaping (`~0`, `~1`).
- `flatten_node` [bug]: empty container records `"array"` (indistinguishable from the string); decoded strings record only children, so raw-only changes aren't detected. Kind-prefixed leaves; record the raw value.
- `flatten_value` / `capture` [bug]: NULL stored as `"NULL"`; options capture turns empty into `"NULL"`. `Option<String>` or sentinel prefix.
- `flatten_doc` [bug]: repeated meta numbered by arrival order (see `meta_sql`).
- `capture(include_actions)` [perf]: unbounded `args LIKE '%id%'`; copies `order_view`; ignores `extended_args`.
- `capture` [product]: `LIMIT 2000` options and `MAX_RELATED = 100` truncate silently. Record `__truncated` or counts.
- `compare` [design]: key union copies `diff`. `union_keys(a, b)`.
- [test] missing: duplicate-meta numbering; `@class` paths; NULL vs `"NULL"`; `/` in keys; empty array vs string; raw change of a decoded string.

### wp/logs.rs
- `paypal_endpoint` [design]: 30-arm match is data. `const ENDPOINTS: &[(&[&str], &str, Option<usize>)]` or TOML.
- `humanize` [bug]: `[New Request]` titles keep a trailing space (`"PayPal request · POST / "`); the test expects it. Trim.
- `humanize` [design]: three `LogSummary { …, None, None, None, None }` literals. `LogSummary::plain(title)`.
- [test] missing: `http://`; unknown PayPal path; absent `[code]`; non-PayPal source with URL; multibyte truncation at 80; endpoint table cases.

### wp/media.rs
- `attachment_url` [bug]: file names not URL-encoded; absolute `_wp_attached_file` and multisite `sites/N` unhandled.
- `entry` [design]: copy of `detail::php_entry`. Move to `php.rs`.
- [test] missing: no-sizes metadata; file without directory + thumbnail; `woocommerce_thumbnail` preference; special characters.

### wp_commands.rs
- `catalog` [perf]: reads and parses the user `entities.toml` and clones built-ins on every command. Cache by `(path, mtime)`.
- `context` [bug]: `wp_cache` never invalidated by `apply_changes` or disconnect; caches `None` (a database that becomes WordPress stays "not"); `lock().unwrap()` panics when poisoned. Invalidate on apply and disconnect; don't cache `None`.
- `plan_change` [design]: 12-arm `database` match → `ChangeRequest::database()`.
- `apply_changes` [design]: executes whatever statement strings the webview sends, bypassing "the frontend never assembles a write" and the destructive check. Store planned changes server-side under an id; apply by id.
- `wp_orders` [design]: `order_type` normalization in the command. `#[serde] enum OrderType`.
- `wp_transient_cleanup_sql` [design]: calls another command handler; duplicates `DeleteExpiredTransients`; unused by the UI. Remove.
- `wp_load` / `wp_expand` / `wp_options` [design]: repeat the `wp_env` prelude; `wp_env` returns a three-tuple → `WpEnv { ctx, catalog, session }`.
- [test]: none. `ChangeRequest::database()`; `OrderType`; catalog error formatting; `WpCache` with `get`/`insert`/`invalidate_session`.

### tests/wp.rs, dev/fixture.sql
- fixture [test]: only HPOS+sync; the legacy store (the `user.orders` crash, legacy `orders_sql`, guests, stats, `SetOrderStatus`, posts-based order view) is never run. Add a `wp_legacy` database; also no multisite or custom-prefix fixture.
- `woocommerce_log.context` [test]: all NULL. Add a serialized context row and a `#47110` message.
- schema [test]: simplified (no `post_content`, `post_date`, `scheduled_date_local`, generated columns, `is_customer_note`, `extended_args`). Mirror real `CREATE TABLE`s.
- `dashboard_summarizes…` [test]: `hpos_drift.is_some()` loose; add `wp_blogs` to the category assertion.
- `peek_summarizes…` [test]: `facts.len() <= 5` and "contains Email" loose; exact list would expose the dropped "Spent".
- `find_value_scans…` [test]: `any(|s| s.ends_with("meta"))`, `count >= 1` smoke-level. Exact groups, counts, ids.
- `lists_sort_by_whitelisted_keys` [test]: with two items asc-first == desc-last is a tautology. Concrete ids.
- `edits_apply_and_undo_cleanly` [test]: `SetOrderStatus` undo doesn't check the posts copy or timestamps; `SetAutoload` only checks plan text; CancelAction, DeleteMeta, DuplicateRow, InsertRow, DeleteLogsBefore never applied; no empty-string/NULL undo fidelity; `DeleteRow` compares `CONCAT(…)` (NULL on any NULL column).
- `snapshots_diff_and_compare` [test]: applies a no-op `UPDATE … WHERE id = 0`; shares `wp_write` rows with the edit test (flaky). Own rows per test.

### WordPress top 10 by impact
1. `load_docs` ignores `rel.when`: any user document on a legacy WooCommerce store fails.
2. Undo loses data: `""` → NULL, binary → NULL, duplicate meta rows collapsed.
3. `sql_string` under `NO_BACKSLASH_ESCAPES` corrupts every written `\`.
4. `apply_changes` runs any statements the UI sends.
5. `edit_serialized`: key-path collisions, `r:N` shifts, `d:inf;`, JSON re-encoding break the lossless guarantee.
6. Order logs: serialized context never matches, `#4711` matches `#47110`, levels off by one tier.
7. Time zones: named zones and DST ignored; dashboard days are UTC.
8. Hidden or relabelled stored data: `user.hidden`, `HIDDEN_POST_TYPES`, trash filtering, label rewrites, tax lines (UI).
9. `detail.rs` and `plan()` can't be unit-tested because logic is mixed with I/O.
10. Tests: no legacy fixture, NULL contexts, simplified schema, loose assertions.

## 4. Frontend (`src/`)

### src/state/store.ts (898 lines)
- `applyChanges` [bug] (verified): no in-flight guard; ⌘S twice or key auto-repeat sends the batch twice. `applying` flag checked and set before the `await`; disable Apply in `PendingTray`.
- `undoLast` [bug]: same race.
- `disconnect` [bug]: leaves `pending`, `applied`, `wp[session/*]`, `selectedCell`, `entityMeta`, `wpNav` behind; StatusBar shows pending changes of a closed session. Purge everything keyed by `sessionId` in one `set`.
- `loadWp` [bug]: mode switch checks only `activeSessionId`, not the database; a late reply for A forces `mode: "wp"` on B. Compare the database; drop stale replies by key.
- `switchDatabase` [bug]: non-WordPress target leaves `mode: "wp"`, `modeChosen` out of sync; two `set` calls. Clear `selectedCell`, `entityMeta`; one `set`.
- `runQuery` [bug]: no running guard, no latest-response check. Request token per tab.
- `runQuery` [design]: `message.replace(/^This statement needs confirmation: /, "")` parses English. `ConfirmationRequired` carries a structured `reason`.
- `closeTab` [bug]: `siblings[Math.min(index, …)]` uses the index in the full `tabs` array; with two sessions the wrong neighbour activates.
- `closeTab` [bug]: clears `selectedCell` even for a non-active tab.
- `loadFavorites` [bug]: no stale guard after `await`.
- `moveFavorite` [bug]: optimistic, no rollback on failure; `map(x => byId.get(x)!).filter(Boolean)` contradicts itself.
- `openFavorite` [design]: renames whatever tab is active. `openQueryTab(sql, title?)`.
- `openTableTab` [design]: `remember()` only on new tabs; re-opening forces `view` and pins.
- `openQueryTab` [design]: `Query ${count+1}` duplicates titles. Per-session counter.
- `openEntity` [design]: Recent title built from the key (`"Shop_order #1"`). Use `entityMeta` title.
- `navigate` [design]: `JSON.stringify` equality depends on key order. `routeKey(route)`.
- `updateTab` [design]: `patch as Tab` lets wrong-kind patches through. `updateQueryTab` / `updateTableTab`.
- `connect` [design]: every connect (including reconnect via launch) opens a new query tab and doesn't reset `wpNav`/`mode`/`selectedCell`.
- `selectedCell` [design]: no tab id; cleared by hand in seven places; survives `dataVersion`. `{tabId, row, col}`, cleared centrally.
- `persistUiState` [bug]: debounced write never flushed on close; errors swallowed. Flush on `beforeunload`; `fail`.
- `nextId` [design]: one counter for tabs, pending, toasts.
- `saveFavoriteScope` [design]: toggles; rename `toggleFavoriteScope`.
- `Dialog` [design]: inline `import("../api").ColumnInfo` (also in Dialogs.tsx).
- `sidebarPanel` [design]: dead `"wordpress"` member.
- `activeSession` helper [design]: duplicates `selectActiveSession`.
- [comment]: stray double blank line after `type Mode`.

### src/api/index.ts, types.ts
- `forgetPassword`, `wpLoad`, `wpExpand`, `wpTransientCleanupSql` [design]: uncalled. Delete with their Rust commands (keep `forget_password` only if a UI will use it).
- `accountOf` [comment, design]: em dash; duplicates Rust's `account()` and `ConnectScreen.describe`. Rust sends `account` on `SessionInfo` and `RecentConnection`.
- `toAppError` [design]: trusts any `{kind, message}`. Validate `kind`.
- `stageAnchors` [design]: hard-codes `page: "main"` while `hooks.ts` passes `PAGE`. (Moves into the stage module.)
- `BrowserSection.list`, `InsightAction.list` [design]: `string` vs a closed set; consumers cast (`as WpList`, `as never`). Type as `WpList` in `types.ts`.
- `InsightAction.kind`, `PeekFact.kind`, `TimelineEvent.level` [design]: unenforced string unions. Rust enums + a generator (`ts-rs`/`specta`, on the roadmap).
- `ListQuery.limit` [design]: Rust `PAGE_MAX = 200` vs `TableTab.MAX_PAGE = 500`. Both limits in Rust, exposed.
- `DocField.label` [design]: autoload in the label text. `autoload` field.
- `DayBucket` [design]: order-named fields reused for actions. Labelled series from Rust.
- `ColumnInfo.extra` [design]: UI substring-matches `auto_increment`/`GENERATED`. `autoIncrement`, `generated` booleans; `TableStructure.primaryKey`.
- `RelatedChip.target` [design]: string protocol. Discriminated union / `WpRoute`.
- `Hint flag` [design]: no `opposite`; `FieldValue` guesses (`FLAG_OPPOSITE`). From Rust.
- header [comment]: "Mirrors the serialized Rust types" without per-section source. Generate or annotate.

### src/commands/index.ts, registry.ts
- `handleKeydown` [bug] (verified): runs shortcuts while a modal or the palette is open (⌘W closes the tab behind Settings; ⌘↵ in the palette runs the editor). Add a `modal` scope / bail when `dialog || paletteOpen` except an allow-list.
- `handleKeydown` [bug]: ignores `e.repeat`.
- `handleKeydown` → `ShortcutField` [bug]: capture phase `stopPropagation` before the field sees it; a bound combo (⌘T) runs instead of recording. Skip targets inside `[data-recording]`. The SettingsDialog test misses it (no window handler installed).
- `document.source` [design]: builds SQL (`` `${idColumn}` = ${id} `` unquoted); EntityScreen, `common.tsx`, FindScreen each build the same filter differently. One `openRow(table, column, value)` action; Rust builds the filter from `{column, value}` in `BrowseRequest`.
- `zoom` [design]: reset values and clamps duplicate Rust defaults and `SettingsDialog` min/max. Rust owns (`sanitized()`).
- `installCommands :: installed` [test]: module-level flag; after `clearCommands()` never re-registers. Re-register when empty.
- `LIST_LABELS` [design]: UI labels in the command module; `lib/crumbs.ts` imports from `commands` (inverted dependency). `lib/wpRoutes.ts`.
- `changes.apply :: when` [bug]: `pending.length > 0` across sessions while apply is per active session.
- `inDb/inWp/hasWp/onEntity` [design]: double `getState()`; duplicate `activeScopes`.
- `bindingFromEvent` [bug]: uses `e.key`; ⌥ combos yield `"˚"`, `"∑"`; shifted punctuation and non-US layouts break. Fall back to `e.code`.
- `normalizeBinding` [bug]: `"Mod++"` unbindable. Parse the last token separately (same in Rust `accelerator`).
- `getOverrides` [design]: exported, unused.
- registry header [comment]: repeats `docs/frontend.md`.

### src/App.tsx, main.tsx
- `DbWorkspace` [bug]: opening the inspector at the bottom dock moves `main` into `.main-stack`; React remounts `TabBar`/`TabPanels` (CodeMirror history lost, TableTab refetches). Stable tree position; toggle the grid template only.
- `DbWorkspace` [css]: `19.231rem` duplicates `.workspace`; `width - 700`, `height - 260` magic. `--sidebar-width` token; named constants.
- `drainLaunches` [bug]: `String(e)` on an `AppError` → `"[object Object]"`. `toAppError(e).message`.
- `App` [design]: nested ternary + `!wpMode && FavoritesBar` → `Workspace` component with early returns.
- `main.tsx window.databee` [design]: the whole store exposed in every build (the `.app` build is not `DEV`). Gate behind `VITE_DEV_HOOKS` set by `scripts/dev`.

### src/lib
- `sql.ts :: statementRanges` [bug]: `/\s|$/.test(sql[i+2] ?? "")` treats `--x` as a comment (MySQL needs whitespace). Explicit check.
- `sql.ts :: sqlString`, `whereEquals` [design]: SQL in the frontend; assumes backslash escaping; FindScreen uses `''`. Move to Rust (structured filter).
- `format.ts :: relativeTime` vs `wpFormat.relativeFromUnix` [design]: two helpers, different outputs; future ISO prints "just now". Merge.
- `formatCount` [design]: `en-US` hard-coded; others call `toLocaleString("en-US")` directly. Use everywhere.
- `compactCount` [design]: `Sidebar.compact` and `Dashboard.compact` differ. One helper.
- `theme.ts :: useResolvedTheme` [bug, perf]: new `MediaQueryList` per render → resubscribe per render of every QueryTab. `useSyncExternalStore` with a module-level query.
- `theme.ts :: rowHeightFor` [css]: 1.75 and 2.1667 unnamed.
- `crumbs.ts :: ENTITY_PARENT` [design]: duplicates Rust entity config and `EntityScreen.parent`. Rust returns `parent`.
- `crumbs.ts :: statusLabel` [product]: strips `wc-` and shows only the label. Show the raw slug.
- `crumbs.ts :: routeCrumbs` [design]: undocumented `params.name`.
- `wpFormat.ts :: absoluteFromUnix` [design]: browser locale/zone; ignores `gmtOffsetSecs` and `utc`. Rust formats, or pass the offset.

### Components
- `Tabs.tsx :: TabPanels` [perf]: each editor keystroke re-renders every mounted tab. `memo(QueryTab)`, `memo(TableTab)`.
- `Tabs.tsx` [design]: `hidden` + `display:none` redundant; `role="tab"` divs without `tabIndex`/arrow keys; nested ternary icon → `TabItem` + `tabIcon(t)`.
- `Tabs.tsx` [css]: "⌘T" hard-coded; `style={{marginLeft: 4}}`. `bindingLabel(bindingFor("tabs.new"))`; CSS class.
- `QueryTab.tsx` [bug]: `"⌘↵ statement · ⇧⌘↵ all"` hard-coded, stale after rebinding. Derive from `bindingFor`.
- `QueryTab.tsx :: useCompletionSchema` [bug]: never refreshes after DDL. Depend on the schema tables or `dataVersion`. Errors replaced with `{}` silently; log.
- `QueryTab.tsx` [comment]: `eslint-disable-next-line` with no ESLint (also TableTab, CommandPalette, useWpData). Add ESLint + `react-hooks` or delete.
- `QueryTab.tsx` [design]: four-way nested ternary results area → `ResultsPane` with early returns. `ErrorPanel` imported by eight files from QueryTab → `components/ErrorPanel.tsx`.
- `QueryEditor.tsx` [design]: unused `ReactCodeMirrorRef`; placeholder has an em dash and "⌘↵".
- `TableTab.tsx :: DataView.load` [bug]: no stale guard; fast paging lets an older page overwrite a newer one.
- `DataView` [bug]: always fetches twice (first with `primaryKey: null`, so an ID filter runs without the shorthand and can flash an error). Wait for the structure, or detect the PK in Rust.
- `DataView` [design]: `i.name === "PRIMARY"` MySQL semantics in UI → `TableStructure.primaryKey`.
- `DataView` + `StructureView` [perf]: `describeTable` twice per tab. One `useTableStructure` or store cache.
- `DataView` [bug]: `describeTable` failure swallowed; editing silently disabled.
- `DataView :: rowActions` [design]: id column case-insensitive, PK case-sensitive.
- `DataView` [design]: ~200 lines: loading, filter form, pagination, edit wiring. `useBrowse(tab)`, `FilterBar`, `Pager`, `useRowEditing`.
- `DataView :: pageSize` [design]: fallback 200 vs `rowLimit` 1000; `MAX_PAGE` duplicates Rust.
- `DataView` [css]: inline absolute-positioned filter icon copied in Sidebar → `SearchInput` component.
- `StructureView` + Sidebar/StatusBar/Inspector/FieldValue/SettingsDialog [bug]: `clipboard.writeText().then()` without rejection handler. `copyText(text, what)` helper.
- `ResultsGrid.tsx :: selected/editing` [bug]: not reset when `rows` change; ⌘C/arrows index past the end and throw. Reset and clamp.
- `ResultsGrid.tsx` edit [bug] (verified): editing a `bytes` cell pre-fills the `0x…` preview and Enter writes it. Disable editing for `bytes` (Rust decides editability).
- `ResultsGrid.tsx` [perf]: subscribes to the whole `settings`; no column virtualization for 100+ columns. Narrow selectors; horizontal virtualizer.
- `ResultsGrid.tsx :: startResize` [bug]: window listeners leak on unmount mid-drag.
- `ResultsGrid.tsx` [css]: `ROW_NUMBER_WIDTH = 52` px (doesn't zoom); `fitWidth` magic (0.6, 0.8, 64, 20, 380). rem constants.
- `ResultsGrid.tsx :: onKeyDown` [design]: only arrows and ⌘C; no Enter-to-edit, Home/End, PageUp/Down. Handler table outside render.
- `ResultsGrid.tsx :: initialRect` [comment]: unexplained `{1000, 800}`; `manual` JSDoc inside the body.
- `Sidebar.tsx :: TablesPanel` [perf]: `useStore(selectActiveTab)` re-renders the tree per keystroke. Select the active table key only.
- `TablesPanel` [design]: `visibleTables` twice per db; database filter duplicates Toolbar → `selectVisibleDatabases`.
- `TablesPanel` [design]: "New query: SELECT …" builds SQL, `${db}` unescaped. Ask Rust (or escape).
- `TablesPanel` [design]: `treeitem` rows without keyboard support.
- `HistoryPanel` [bug]: refetches on `runningCount`; no stale guard; `clearHistory` errors swallowed. `historyVersion`; `fail`.
- `HistoryPanel` [perf]: not virtualized. keys `executedAt + sql` collide; id from Rust.
- `Sidebar.tsx` [css]: inline px styles; "⌘⇧F" hard-coded.
- `Splitter.tsx :: start` [bug]: listeners leak and `resizing-x` sticks on `body` if unmounted mid-drag.
- `Splitter.tsx :: useWindowSize` [perf]: one resize listener per consumer. `useSyncExternalStore`.
- `StatusBar.tsx :: pending` [bug]: counts all sessions. Filter by active.
- `StatusBar.tsx` [design]: `querySelector(".tray .btn-primary")?.focus()` couples to another component's CSS. HPOS label mapping duplicated with Dashboard `syncLabel`.
- `StatusBar.tsx` [perf]: subscribes to whole `settings` and active tab object.
- `Toolbar.tsx` [design]: new-query button duplicates `tabs.new`; titles hard-code "(⌘N)" etc. Subscribes to `settings` for `showSystemSchemas`.
- `FavoritesBar.tsx` [bug]: can't drop at the last position. End drop zone. "(⌘D)" hard-coded.
- `CommandPalette.tsx` [bug]: `setSearching(true)` never reset when the effect is cancelled ("searching…" sticks). Reset in cleanup.
- `CommandPalette.tsx` [bug]: arrow navigation doesn't scroll into view; `index` not clamped when `items` shrink.
- `CommandPalette.tsx :: Item.goAlt` [comment]: JSDoc says ⌘↵ opens the source row; entity items have no `goAlt`.
- `CommandPalette.tsx` [design]: option hit navigates to the list without selecting (FindScreen same). Rust returns a `WpRoute` per hit. `setTimeout(focus)` without cleanup → `autoFocus`. Reimplements overlay/Escape instead of `Modal`. Split `usePaletteItems(query)`, `PaletteList`.
- `ContextMenu.tsx` [bug]: `key={item.label}` collides. No focus/arrow keys; height estimate counts separators. `onClose` in effect deps re-adds listeners per render (ref).
- `Dialogs.tsx :: InsertRowDialog` [bug]: "Type NULL" makes the string `"NULL"` impossible; empty fields dropped so `""` into NOT NULL without default impossible. Explicit NULL/default toggle per field. `extra.includes(...)` → Rust flags.
- `Dialogs.tsx` [design]: `spec: Parameters<typeof accountOf>[0]` → `ConnectionSpec`; `display: contents` inline ×3; connect modal lacks `labelledBy`.
- `Modal.tsx` [design]: no focus trap, no focus restore; Escape bubble-phase while shortcuts fire in capture.
- `PendingTray.tsx` [bug]: Apply/Undo have no busy state. Bind to `applying`. "Apply (⌘S)" hard-coded; `statements.join(";\n") + ";"` formats SQL in the UI.
- `Inspector.tsx` [design]: `len > 16 ? "…"` duplicates `cellText` with a different threshold; `showRaw` not reset on selection change; `MAX_DECODE_CHARS` policy in UI.
- `ValueTree.tsx :: LeafEditor.submit` [design]: unreachable `case "bool"`. `Row` [perf]: depth < 2 auto-open, not virtualized (thousands of entries). `vt-line` no keyboard.
- `Toasts.tsx` [css]: `width: "1.385rem", height: 18` mixed units.
- `ConnectScreen.tsx :: ConnectForm` [bug]: mounts before settings load; `toForm(undefined, 3306)` ignores `defaultPort`. Re-key on settings.
- `ConnectScreen.tsx :: validate` [design]: duplicates Rust validation with a different default. `connect` returns `invalid_input`.
- `ConnectForm :: removeRecent` [bug]: error swallowed, UI removes anyway. recents `role="button"` divs without keyboard; title fallback differs from `data-stage-label` fallback.
- `SettingsDialog.tsx :: TextField(number)` [bug]: every in-range keystroke commits (typing 100000 saves 1, 10, 100…); racing `update_settings`. Debounce or commit on blur/Enter plus spinner `input`.
- `ShortcutsPage` [bug]: `c.id.includes(needle)` with lowercased needle vs camelCase ids.
- `SettingsDialog.tsx` [design]: 401 lines → `settings/controls.tsx`, `ShortcutsPage.tsx`, `pages.tsx` (registry), `SettingsDialog.tsx`; Presentations page moves to `src/stage/`.
- `PresentationsPage` [comment]: "§10 of the Stage protocol" external reference; "New Token" capitalization.
- `PageBody :: logging` [design]: log-level list duplicates `LogLevel`; min/max duplicate Rust.
- `wp/WpWorkspace.tsx` [design]: `routeTitle` exported unused; `ATTENTION` statuses decided in UI (overlaps `ui.STATUS_TONES`, `crumbs.statusLabel`): Rust returns `severity` per facet; Tools group special-cased by title string; unreachable `!wp` branch; `Header` hard-codes "⌘[", "⌘]", "⌘K" and subscribes to all of `wpNav`.
- `wp/Dashboard.tsx :: rowItems` [product]: `t.name.replace(ctx.prefix, "")` shows table names without prefix (replaces first match anywhere). Raw name.
- `Dashboard.tsx` [design]: `syncLabel` nested ternary → `hposLabel`; `"healthy < 800 KB"` threshold repeated in `Settings.tsx` while Rust returns `autoloadHealthy`; ~140 lines JSX → `DashHeader`, `StatTiles`, `ActionSchedulerCard`, `RecentOrdersCard`; lucide imported twice; `InsightRow key={i.text}`, `a.list as never`; `HookGroup` abuses `ListQuery.kind` and `useWpData` with a null loader → `enabled` option; `setMode("db") + openTableTab` duplicated in five places → `openInDatabase`.
- `wp/Debugging.tsx` [bug]: `wpSnapshotDelete().then(reload)` without catch, no confirmation, `diff` not cleared. [design]: `String((e as {message?}).message ?? e)` ×3 → `toAppError`/`fail`; default pattern `woocommerce\_ppcp%` plugin-specific; `DiffTable`/`CompareScreen` duplicate table; em dash in "No differences — nothing changed".
- `wp/FindScreen.tsx :: openRow` [bug]: `''` escaping, `007` unquoted (matches `7`). Shared action. `key={g.table}` collides.
- `wp/useWpData.ts` [design]: spread `deps` + eslint-disable; stale closures unenforced. Take a `key` tuple. [bug]: previous database's `data` shown on switch; `loading` stuck `true` when `!sessionId || !database`.
- `wp/ui.tsx :: STATUS_TONES` [design]: status semantics in UI. `Avatar.onError` mutates DOM style.
- `wp/Peek.tsx :: cache` [bug]: key lacks the database; never cleared on `dataVersion`. JSDoc claim false.
- `wp/Settings.tsx :: OptionGroup` [bug]: autoload from `label.endsWith("· autoload")`; table name `${prefix}options` built in UI (wrong on subsites; `OptionsOverview.table` exists). [perf]: transient list not virtualized.
- `wp/Lists.tsx` (464 lines) [bug]: filters in local state not the route (back/forward loses them, crumbs drift); filter change fetches twice (old offset then 0) and races; `onKey` scrolls to the previous row and on narrow windows ArrowDown opens the entity; content chips show `"post"` active while sending `kind: null`; `"All ${label}s"` → "All kindss". [design]: five parallel switches/records typed `unknown` → one typed config per list; `select` `useCallback` pointless; `button role="row"` without `gridcell`. Split: `lists/config.ts`, `lists/cells.tsx`, `ListToolbar`, `ListTable`, `DetailPane`, `useListQuery`.
- `wp/FieldValue.tsx` (308 lines) [design]: `PIVOTS` (field-name → list) and `text.startsWith("wc-")` are naming heuristics in UI → Rust `pivot` per field; `FLAG_OPPOSITE` → Rust; `formatBytesShort` duplicates `formatBytes`; misplaced JSDoc; six-branch chain + switch → `FieldBody` with early returns + `FieldActions`; `Parameters<ReturnType<…>["navigate"]>[0]` → `WpRoute`; `LongText` `TextEncoder` per render.
- `wp/BreakdownChart.tsx` [design]: redundant `onKey` on buttons; `splitRows` silent clamp.
- `wp/views/EntityScreen.tsx` [bug]: "updated Xs ago" computed in render (always 0s); `changed` never cleared; Compare prompt ignores non-numeric input silently; Source row duplicates `document.source` unquoted; parent mapping duplicates `crumbs.ENTITY_PARENT`.
- `wp/views/OrderView.tsx` [bug]: `items.filter(i => i.kind !== "tax")` hides stored tax lines; meta key names computed from HPOS mode in UI (`_payment_method` vs `payment_method`), `_ppcp_*` hard-coded → Rust returns `[sourceKey, value]`; `${prefix}woocommerce_order_itemmeta` built in UI; `ORDER_STATUSES`/`SUBSCRIPTION_STATUSES` hard-coded (custom statuses missed) → Rust returns distinct statuses; logs fetched with a local effect (live/⌘R never refresh) → `useWpData`; unindented JSX; `getElementById("order-timeline")` → ref; `label === "Total"` styling by English; fourth `Row` component → shared `KvRow`.
- `wp/views/OtherViews.tsx` (246 lines) [design]: `OrdersMini` duplicates Dashboard rows and `OrderCells` → `OrderRow`; `ActionView` log level by `/fail|error|exception/i` and re-implemented `Timeline` with `key={i}`; allowed actions (Retry for complete) decided in UI → Rust; `CustomerView.onTarget` string protocol; `expired` duplicated; `changed` drilled through every view. One file per view.
- `wp/views/common.tsx` [bug]: `TimelineItem.openRow` unquoted WHERE; `e.kind.replace("_"," ")` first only; `Timeline key={at-i}` shifts when logs merge (stable id from Rust); `RelationStrip`/`AddressCard` `key={i}`/`key={l}` collide; stale `AllFields` JSDoc.

### CSS
- `theme.css`: `--tile-*` (14 legacy treemap tokens) unused; dark tokens copied twice (~50 lines each) → one copy; header em dash; missing `--sidebar-width`, `--wp-nav-width`, `--radius-pill`, hairline token.
- `app.css` dead selectors: `.doc`, `.doc-head`, `.doc-section`, `.relation`, `.relation-item`, `.wp-panel` (+ `dl/dt/dd`), `.stat-row`, `.stat .n/.l`, `.bar`, `.legend`, `.confidence`, `.btn-ghost`, `.tab .spinner`, `.panel`, `.icon-btn.on`, `.badge.tone-warning`.
- `app.css` duplicates: `.kv` twice with conflicting columns, `.kv > .k/.v` vs `.kv .k/.v`; `.insight`, `.wp-content`, `.grid-hcell` (×3), `.palette-list li` defined twice.
- `app.css` hard-coded colors: `#fff` in `.btn-danger`, `.btn-live`, `.avatar`, `.switch::after`; `rgba(0,0,0,…)` in `.switch`, `.segmented`, `.overlay`. Tokens `--on-accent`, `--shadow-control`, `--scrim`.
- status vs chart colors: `.pill.tone-*` uses `--chart-*` while `.nav-badge`/`.wp-badge` use `--danger`/`--warning`. One role.
- magic px: `.donut-value` 9px, `.donut-label` 5px (SVG units; comment), `.col-resize` `-3px`/`6px`, many `2px` gaps.

### Tooling
- `package.json lint` [design]: no ESLint, yet `eslint-disable` comments exist. Add ESLint with `react-hooks/exhaustive-deps` (and remove the disables by fixing the effects).
- `tsconfig.json` [design]: `noUncheckedIndexedAccess` off (hides `rows[selected.row]`). Turn on.
- `vite.config.ts` [design]: no `restoreMocks`/`unstubGlobals`; tests mutate `innerWidth` and fake timers without restoring.

### Frontend tests
- `ErrorBoundary.test.tsx` [test]: asserts `api.logError` was called (log output; forbidden). Keep the visible fallback and recovery.
- `QueryTab.test.tsx :: "runs only the statement under the cursor"` [test]: `/^SELECT \d$/` accepts either statement. Expect `"SELECT 2"`.
- `QueryTab` cancelled confirmation, `SettingsDialog toHaveBeenCalledTimes(1)`, `Peek toHaveBeenCalledTimes(1)` [test]: call counts. Assert visible outcomes.
- `SettingsDialog.test.tsx :: "records … overrides"` [test]: no capture-phase `handleKeydown` installed; hides the real bug. Render `<App/>`.
- `BreakdownChart.test.tsx` [test]: asserts CSS classes via `document.querySelector`. Centre text only.
- `WpMode.test.tsx` [test]: `.closest(".stat-tile")`, `.closest("tr")`; `innerWidth` mutated and never restored (order-dependent); fake timers not restored on failure.
- `FieldValue.test.tsx` [test]: loose regex alternation. Exact text.
- `StatusBar.test.tsx` [test]: `{id, sessionId} as never` seed. Full fixture.
- many [test]: assert `useStore.getState().tabs.at(-1)` etc.; prefer visible results.
- `test/apiMock.ts` [test]: "harmless defaults" but `connect`, `execute`, `describeTable`, `browseTable`, `wpDashboard`, `wpOrders`, `wpUsers`, `wpProducts`, `wpSessions`, `wpContent`, `wpView`, `wpSnapshotTake` return `undefined`. Typed defaults; `EMPTY_UI_STATE` from types.
- `test/wpFixtures.ts` [test]: `Date.now()` at module load. Fixed `now`.
- Untested, worth tests (through the UI): store (disconnect purges, closeTab neighbour, ⌘S twice applies once, stale `loadWp`, failed reorder rollback); Tabs (middle-click, double-click pin, preview title, close doesn't activate); QueryEditor (⌘↵ no newline, completions, contents survive bottom-dock inspector); CommandPalette (Recent, arrows + scroll, ⌘↵, Escape, "searching…" clears, option hit); Toolbar (database switch reloads tables, disconnect, session select); Inspector/ValueTree (decode + Raw, bytes preview, banners, expand, bool edit); ContextMenu (Escape, outside click, item runs); Dialogs (InsertRow values, NULL, auto-increment hidden); PendingTray (apply refreshes, no-undo badge, active session only); commands (zoom limits, `view.wp` disabled, `document.source`, `wp.snapshot`, `favorites.add`, `changes.undo`); Dashboard (drift label, no-WooCommerce empty states); Debugging (options snapshot, delete + error); FindScreen (zero hits, option hit, non-numeric id); Lists (products, actions, sessions, content, pagination, kind select, close pane, back keeps filters); WpWorkspace (crumbs, back/forward disabled); views (Customer, Product, Action retry/cancel, Session, Generic, Compare prompt, Snapshot, Source row).

### Frontend top 10 by impact
1. Apply and undo can run twice (no in-flight guard, key repeat).
2. Editing a binary cell writes the hex preview into the column.
3. App shortcuts fire behind modals and the palette, and block shortcut recording.
4. SQL filters built in the frontend four inconsistent ways.
5. Disconnect leaves session state behind; StatusBar counts all sessions.
6. Bottom-dock inspector remounts all tabs.
7. No stale-response guards (browse double fetch, `runQuery`, `loadFavorites`, `loadWp`, `useWpData`, Peek cache key).
8. Frontend parses or invents DB semantics (autoload label, HPOS key names, `_ppcp_*`, status lists, log-level regex, flag opposites, "PRIMARY"/"GENERATED"); tax lines hidden; prefixes stripped.
9. List filters in local state, not the route; double fetch; arrow keys open entities on narrow windows.
10. Re-render and listener costs (per-keystroke re-render of all tabs, sidebar and status bar; `matchMedia` resubscribe; whole-settings subscriptions; no column virtualization).

---

# Part 2: Plan

## Decisions already made (do not re-litigate)

Owner decisions:
- **Modules.** Every optional capability is a module with one install line per side. Removing one is "delete the folder, drop the line". The core never imports a module; modules may import the core.
- **IPC rule.** `invoke`/`listen` only in files named `api.ts`. Each module owns `api.ts` + `apiMock.ts`; the core keeps `src/api/index.ts`.
- **Types.** TypeScript types are generated from Rust with `ts-rs` per module. The hand-written `src/api/types.ts` goes away.
- **Scope.** Everything in Part 1, in four shippable phases.

Technical decisions (from the review; the implementer follows them):
- Rust optional features are inline **Tauri plugins** (`tauri::plugin::Builder`), each with its own commands, managed state, settings file and `capabilities/<name>.json`. Verified against tauri 2.12.4: plugin `setup` runs before the app `setup`, so `AppState` is managed with `tauri::Builder::manage` before `.plugin(..)`; JS calls `invoke("plugin:<name>|<cmd>")`; ACL needs an `InlinedPlugin` entry in `build.rs` with `DefaultPermissionRule::AllowAllCommands`.
- The driver seam is a `Database` trait (`async_trait`, already a transitive dependency) with `Box<dyn Database>` in `Session`, plus a `Dialect` value. Not an enum.
- Planned changes are stored server-side per session (`Plans`) and applied by id. The webview never sends SQL to execute.
- `Session.ext: Extensions` (a `TypeId` map) lets a module keep per-session data that dies with the session.
- Keep `panic = "unwind"` (drop `panic = "abort"`); audit runtime-reachable `unwrap`/`expect`.
- New dependencies allowed: `async-trait`, `ts-rs` (feature `serde-compat`), `zeroize`, `getrandom`, `chrono-tz`, `rust_decimal` (money), `eslint` + `eslint-plugin-react-hooks` + `typescript-eslint` (dev). Removed: `tauri-plugin-single-instance`, clap `env` feature.
- `MemoryStore` and `FakeDatabase` compile under `#[cfg(any(test, feature = "test-util"))]`; the crate depends on itself with `test-util` in `[dev-dependencies]` so `tests/` sees them.
- User-visible config effects: `settings.json` loses the `stage` key (ignored if present); Stage settings move to `stage.json`; `StageStore::open` copies a legacy `settings.json.stage` object once so the registered token survives. `snapshots.json` keeps its name and format.

## How to execute a step

1. Read the finding bullets referenced by the step (Part 1 section names) and the files named.
2. Make the change. Keep each step to one commit; if a step says "may split", split at the marked point.
3. Tests: hand new or changed tests to `write-rust-unit-test` / `write-ts-unit-test` with the behaviors listed under **Tests**. Run with the `ci` agent. Integration tests (`#[ignore]`) need `bun run test:db` through `ci`.
4. Update the docs page named under **Docs** in the same commit.
5. `ship-step`.

Never touch the user's DDEV databases. Writes in manual checks go to the Docker fixtures (`wp_write`, `databee_test`).

---

## Phase 1: correctness and security, structure-independent

These fixes touch files in their current locations. None depends on the seams.

### 1.1 Single-instance handoff
Findings: lib.rs (`run`, `config_dir`, IPC closure), ipc.rs (all).
- `ipc.rs`: `enum Forward { Delivered, NoInstance, Rejected }` from `try_forward`; `bind` connects first and unlinks only on `ConnectionRefused`; `serve` wraps the reader in `.take(64 * 1024)` and `timeout(5 s)`, checks `peer_cred().uid == libc::getuid()` (or `nix`), sleeps 100 ms after an accept error, logs a failed ACK; `IpcMessage::Eval` behind `#[cfg(debug_assertions)]`; manual `Debug` for `IpcMessage` that redacts the password.
- `lib.rs::run`: `config_dir()` exits with a message when there is no home dir; `Settings::peek(dir)` (read-only, no quarantine) for `default_port`; start only on `NoInstance`, otherwise print and exit 2; `flock` on `<config>/databee.lock` before `bind`.
- Fix the `ipc.rs` module doc and the `--url` help text about passwords in argv.
- **Tests** (`ipc.rs`, `cli.rs` existing modules): stale socket without listener → `NoInstance` and `bind` replaces it; malformed line → `Rejected`; `Error`/`Activate` round trips; two clients at once; oversized line refused; `peek` never renames a corrupt file.
- **Docs**: `docs/backend.md` "CLI and single-instance handoff".

### 1.2 JSON files and the generic store
Findings: store/ (all), settings.rs `update`.
- `json_file.rs`: every read error quarantines (timestamped `<name>.<ts>.corrupt`, logged); `save` opens the temp file with mode 0600, writes, `sync_all`, renames; the config dir is created 0700; delete `JsonFile::path`.
- New `store/json_store.rs`: `JsonStore<T: Serialize + DeserializeOwned + Clone + Default>` with `open(path)`, `read(|&T| -> R)`, `update(|&mut T| -> R) -> AppResult<R>` (clone, mutate, save, then swap under one lock). Migrate `settings`, `keybindings`, `recents`, `favorites`, `history`, `snapshots`, `ui_state` onto it; `SettingsStore::update` returns `(previous, saved)`.
- `history.rs`: cap stored SQL at 64 KiB, dedupe by hash. `favorites.rs`: `NewFavorite` input, `InvalidInput` for unknown ids, dedupe on update. `snapshots.rs`: cap per `(account, database)`; metadata index + one file per snapshot can wait for Phase 3. `ui_state.rs`: rename fields, drop the comments, prune in `remove_recent`. `keybindings.rs`: `sanitize` bindings; `rsplit_once('+')` in `accelerator`.
- **Tests**: json_file (non-UTF-8 quarantined, unreadable file, second corruption keeps the first backup, no stray `.tmp`); each store's missing cases listed in Part 1 §1 store/.
- **Docs**: `docs/backend.md` "Persistence".

### 1.3 Database layer correctness
Findings: db/mod.rs, db/lexer.rs, db/safety.rs, db/cell.rs, db/id_filter.rs, error.rs, db/mysql.rs (the `[bug]` items only; the split is 2.A3/3.x).
- `db/mod.rs` → `db/types.rs` (DTOs), `db/browse.rs` (`browse_sql` returning `(String, Option<AppliedFilter>)`, `WHERE ({filter}\n)`, reject unterminated quote/comment, comment-only = blank, `FilterKind` enum, `limit >= 1`), `db/dialect.rs` (`Dialect::MYSQL` with `quote_ident`, `sql_string` moved from `wp/mod.rs`, `table_ref`, `LexerOptions`, `system_schemas`). Keep `db::quote_ident` and `wp::sql_string` as thin wrappers until 2.A3.
- `lexer.rs`: `/*!` bodies are code; `$`, bytes ≥ 0x80, digits in identifiers; `@name` one piece; parenthesis depth on pieces; iterator-based `scan`; module doc lists `NO_BACKSLASH_ESCAPES`, `ANSI_QUOTES`, `DELIMITER` limits.
- `safety.rs`: `WHERE` only at depth 0; `CREATE OR REPLACE`, `RENAME TABLE` flagged; `returns_rows` → `lexer.rs::classify` with the missing statement kinds.
- `error.rs`: map `Protocol`, `WorkerCrashed`, codes 2006/2013/1053/1927/4031 to `Connection`; `AuthFailed` Display `"{0}"`; `Timeout(Duration)`; doc `code: string | null`.
- `mysql.rs` bugs (in place): `SELECT DATABASE()` inside the timeout and non-fatal; `reconnect_editor` with timeout and logged; `use_database`, `databases`, `tables`, `structure`, `completion` under `query_timeout`; `kill` returns `AppResult`, uses a one-off connection; `apply` `close_on_drop()` on timeout with "outcome unknown after COMMIT"; column recovery only when `affected_rows == 0` and the post-CTE statement is `SELECT`; `statement_cache_capacity(0)` on the editor; `structure` DDL `Option<String>`; `try_join!` for the three structure queries and for pool+editor connect.
- `cell.rs`: hex via one `String`; `id_filter.rs`: leading digit required, `partition`, `pub(super)`.
- `Cargo.toml`: `crate-type = ["rlib"]`, drop `panic = "abort"`, drop `tauri-plugin-single-instance` and clap `env`, delete both template comments. `tauri.conf.json`: CSP, drop `icon.ico`. `main.rs`: drop the Windows comment and attribute.
- **Tests**: every missing case listed in Part 1 §1 for lexer, safety, browse, cell, id_filter, error; `run_script` result assembly as a pure fold (extract `fold_events(events) -> Vec<ResultSet>`); the column-recovery decision function.
- **Docs**: `docs/database.md` modules table and "Running a query".

### 1.4 Secrets, passwords, logging
Findings: connection.rs, launch.rs, secrets.rs, db_commands.rs `connect`/`execute`, commands.rs `log_frontend_error`, dev.rs.
- `connection.rs`: `Secret` over `Zeroizing<String>`, `Deserialize` impl, `PartialEq` under `cfg(test)`; `account()` percent-encodes `@` and `:` in the user.
- `launch.rs`: timestamped one-shot passwords (5 min), direct `Launch` destructure; `LAUNCH_EVENT` moves here.
- `db_commands.rs`: `ConnectArgs.password: Option<Secret>`; take the launch password whenever `request_id` is present, only consume on success; `execute` logs kind/code at info and the message at debug; `cancel_query` propagates errors. (The move into `session::open` is 2.A4.)
- `commands.rs`: `log_frontend_error` truncates to 4 KiB and escapes newlines. `dev.rs`: `#[cfg(debug_assertions)]` module, `is_finite()` check.
- `secrets.rs`: `PasswordSource::Unavailable(reason)`; `MemoryStore` gated (the folder split is 2.A2).
- **Tests**: connection (identity without database, socket display names, no `password` key in JSON); launch (listed cases); secrets (failing store, delete missing, `secret()` per variant); dev (NaN/inf, whitespace, negative width).
- **Docs**: `docs/backend.md` "Secrets", "Errors".

### 1.5 Frontend correctness, in place
Findings: store.ts, commands/index.ts, registry.ts, App.tsx, ResultsGrid, TableTab, Lists, CommandPalette, Splitter, ContextMenu, Peek, useWpData, ConnectScreen, SettingsDialog `TextField`, Debugging, FavoritesBar, PendingTray, StatusBar.
- `store.ts`: `applying` guard on `applyChanges`/`undoLast` (PendingTray disables buttons); `disconnect` purges everything keyed by session in one `set`; request tokens on `runQuery`, `loadFavorites`, `loadWp` (compare session **and** database); `switchDatabase` one `set` and clears `selectedCell`/`entityMeta`; `closeTab` neighbour within siblings and `selectedCell` only when the active tab changes; `moveFavorite` rollback; `persistUiState` flush on `beforeunload`; `openQueryTab(sql, title?)` with a per-session counter; `updateQueryTab`/`updateTableTab`; `selectedCell` as `{tabId, row, col}` cleared centrally; `routeKey`; drop dead `"wordpress"` panel member, `activeSession` helper, stray blank line, `saveFavoriteScope` → `toggleFavoriteScope`.
- `commands/index.ts`: `handleKeydown` bails when `dialog || paletteOpen` (allow-list: `app.settings` close, zoom), skips `e.repeat`, skips targets inside `[data-recording]`; `changes.apply when` filters by active session; `installed` flag removed; `inDb/inWp/...` derived from `activeScopes`. `registry.ts`: `bindingFromEvent` uses `e.code` when Alt is held or the key is punctuation; `normalizeBinding` handles `+`; delete `getOverrides`; shorten the header.
- `App.tsx`: stable `main` position (grid template toggles only); `toAppError(e).message` in `drainLaunches`; `Workspace` component with early returns.
- `ResultsGrid`: reset `selected`/`editing` on `rows` change and clamp; no editing for `bytes`; listener cleanup in `startResize`; narrow settings selectors. `TableTab`: stale guard; wait for the structure before the first browse; show `describeTable` errors; one `useTableStructure` shared with `StructureView`. `Lists`: filters in `route.params` via `replaceRoute`, offset reset inside setters, scroll in an effect, selection only when `split`, "All kinds" wording, chips reflect `kind: null`. `CommandPalette`: reset `searching`, scroll into view, clamp index, `autoFocus`. `Splitter`/`ContextMenu`: listener cleanup, `onClose` in a ref, index keys. `Peek`: cache key includes the database; cleared on `bumpData`. `useWpData`: key tuple; reset `data` on session/database change; `loading=false` on early return. `ConnectScreen`: re-key the form on settings; surface `removeRecent` errors. `SettingsDialog.TextField`: commit on blur/Enter plus spinner `input` event, debounced; `ShortcutsPage` lowercases ids. `Debugging`: catch on delete, confirm, clear `diff`. `FavoritesBar`: end drop zone. `StatusBar`: pending by active session. `theme.ts`: `useSyncExternalStore` for `matchMedia`. `sql.ts`: `--` needs whitespace. `copyText(text, what)` helper replaces every bare `clipboard.writeText().then()`.
- **Tests**: the "untested, worth tests" list in Part 1 §4 for store, Tabs, CommandPalette, PendingTray, TableTab stale guard, ResultsGrid bytes/reset; fix the loose tests listed under "Frontend tests" (ErrorBoundary log assertion, `/^SELECT \d$/`, call counts, SettingsDialog rendering `<App/>`, BreakdownChart classes, WpMode `innerWidth`/timers restore, FieldValue regex, StatusBar seed); `apiMock` typed defaults; fixed `now` in `wpFixtures`.
- **Docs**: `docs/frontend.md` "Store" conventions.

### 1.6 WordPress correctness, in place
Findings: wp/mod.rs `load_docs` `rel.when`, `sql_string`; changes.rs undo bugs; detail.rs `log_level`, `order_logs`; peek.rs `truncate(5)`; hints.rs `money`; context.rs timezone; wp_commands.rs `context` cache.
- `load_docs`: filter relations by `rel.when.applies(ctx)`. `changes.rs`: undo uses `cell_str` (keep `""` vs NULL), no undo for `Bytes`, key-column change uses the new key, refuse `SetMeta` with `n > 1` until `MetaDef.id` exists (3.D3), refuse no-op `DeleteMeta`, `DeleteLogsBefore` cutoff literal, `edit_serialized` no `trim`, `php_new(Float)` PHP grammar, refuse edits with `Ref` nodes after the edited position. `detail.rs`: `log_level` ranges, serialized-PHP context match, word-boundary id match with escaping, `action_view` prefers `extended_args`. `peek.rs`: cap 7 with order stats first. `hints.rs`: `rust_decimal` money, reject non-finite. `context.rs`: probe `timezone_string`, `chrono-tz` conversion helper used by `hints::time` and `dashboard` day buckets. `wp_commands.rs`: don't cache `None`; invalidate on `apply_changes` and `disconnect` (full move into `Session.ext` is 2.A5); `sql_string` escaping follows `@@sql_mode` read once per session (lands with `Dialect` in 2.A3; until then read it in `wp::detect` and pass a flag).
- **Fixture**: `dev/fixture.sql` adds a serialized `woocommerce_log.context` row and a `#47110` message; a `wp_legacy` database (posts-based orders, no `wc_orders`).
- **Tests**: `log_level` table; `plan()` unit tests over `FakeDatabase` come in 2.A3; until then integration tests in `tests/wp.rs` for empty-string undo, `wp_legacy` user document, order-log context match.
- **Docs**: `docs/wordpress.md` "Writes", "Detection".

---

## Phase 2: module seams and big-file splits

### 2.A Rust seams

**A1 Startup shape.** Findings: lib.rs `start_app`, commands.rs `update_settings`/`apply_window_theme`, menu.rs. Build `AppState` before `tauri::Builder` and `.manage(state)` first; extract `init_logging(level)`, `apply_dev_frame`, `start_ipc(handle, socket)` and `launch::on_ipc_message(&AppHandle, IpcMessage)`; `window.rs` with `apply_window_theme` (logged); `apply_settings(app, previous: Option<&Settings>, saved: &Settings)` used by setup and `update_settings`; the CLI log override kept in `AppState`; non-fatal token/socket errors. `menu.rs`: one `MENU_ITEMS` table (id, label, default binding) replacing `LABELS` + `MENU_DEFAULTS`, rebuild on `set_keybindings`, skip and log unparsable bindings, log `emit` failures. Tests: `MENU_ITEMS` consistency, `is_menu_command`, `IDENTIFIER == tauri.conf.json.identifier`, `apply_settings`. Docs: `docs/backend.md` "Startup", "Native menu".

**A2 Secrets folder.** `secrets/{mod,keychain,memory}.rs`; `[features] test-util`; self dev-dependency; the install line in `lib.rs` is the only place a `SecretStore` is chosen. Docs: `docs/backend.md` "Secrets" with the swap recipe (Part 2 §Recipes).

**A3 Driver seam.** `db/mod.rs`: `#[async_trait] trait Database` (signature in the design: `dialect`, `server_version`, `current_database`, `close`, `execute`, `cancel`, `use_database`, `databases`, `tables`, `structure`, `completion`, `browse`, `fetch`, `apply`) and `db::open(spec, password, timeout) -> AppResult<Box<dyn Database>>`; `MySqlSession: Database`; `Session.db: Box<dyn Database>`; every `&MySqlSession` parameter in `db_commands.rs`, `session.rs`, `wp/*.rs` becomes `&dyn Database`; `Dialect` carries `backslash_escapes` read from `@@sql_mode` at connect; `db/fake.rs` `FakeDatabase` (`on(sql_contains, result)`, `failing(..)`, records `applied`). `tests/mysql.rs` keeps using `MySqlSession`. `db/mysql.rs` splits into `db/mysql/{mod,editor,introspect,script}.rs` (findings db/mysql.rs [design]); `with_timeout` → `connect_within` + `query_within`; `FromRow` structs for introspection; `column_meta` helper; `Family` computed once per result set and `cell::decode(family, raw)`. Tests: `connect_options`; `Sessions` with `FakeDatabase`; `fold_events`. Docs: `docs/database.md` "Modules", "Connection model", "Adding another database engine" (now: implement `Database`, add an `Engine`, one arm in `db::open`).

**A4 Session service.** `session::open(state, args) -> AppResult<Arc<Session>>` holds password resolution, Keychain save (via `spawn_blocking`), recents touch, auth-error remapping (`auth_error(source, account, message)` pure); `SessionInfo::new(id, spec, version, db, account)` pure, with `account` sent to the page; `connect` and `disconnect` commands become wrappers; `close()` kills the running query and closes the editor. Tests: `auth_error` table; confirmation gate before session lookup; failed query still in history (with `FakeDatabase`). Docs: `docs/backend.md` "AppState", "Tauri commands".

**A5 Session extensions.** `session.rs`: `Extensions` (`TypeId → Arc<dyn Any + Send + Sync>`, `get_or_insert_with`); `wp/cache.rs::WpCache` (only `Some` cached, `invalidate(db)`) stored in `session.ext`; delete `AppState.wp_cache`. Tests: `WpCache` get/insert/invalidate; cache dies with the session.

**A6 Decode module.** `git mv wp/php.rs decode/php.rs`, `wp/value.rs → decode/value.rs`, `edit_serialized` and `NewValue` → `decode/edit.rs`, `decode_value`/`edit_serialized` commands → `decode/commands.rs` (core, because the Inspector uses them). `PhpValue::get(key)` and `as_text()` replace the five lookup copies. Apply the php.rs/value.rs findings (non-canonical ints, float grammar, lenient false-success, `looks_serialized`, `declaring_class`, raw key bytes, enum kinds, index-based edit paths). Tests: the lists in Part 1 §3 php.rs and value.rs; `edit_serialized` JSON span splice or documented PHP-only claim. Docs: `docs/wordpress.md` "PHP serialize" moves to a new `docs/decode.md` (or a section of `docs/backend.md`); `docs/adapting.md` "Removing the WordPress layer" updated.

**A7 Plan-by-id.** Findings: wp_commands.rs `apply_changes`, `plan_change`; changes.rs `plan` split; store.ts apply. Split at (a)/(b):
- (a) Rust: `db/changes.rs` (generic `ChangeRequest` with `database()`, `Plan`, `plan` dispatching to `plan_set_cell`/`plan_delete_row`/`plan_duplicate_row` (row-count check, no generated columns)/`plan_insert_row`, `reinsert_sql` skipping generated columns, `sql_value(dialect, ..)`), `db/plans.rs` (`Plans` with `add`, `discard`, `take`, `record_applied`, `take_undo`; applied capped at 20), `Session.plans`; core commands `plan_change`, `discard_change`, `apply_changes(ids)`, `undo_batch`; `wp/changes.rs` keeps the WP variants as `WpChangeRequest` with `plan` returning `db::changes::Plan`, `meta_target` shared, one `plan_*` per variant; `wp_plan_change` command (registered in `lib.rs` until D1). Keep the old `apply_changes(statements)` for one commit.
- (b) Frontend: `queueChange` → `planChange` returns `PlannedChange {id, description, statements, canUndo}`; `applyChanges(ids)`; `undoLast` → `undoBatch(batchId)`; delete the old command and `wp_transient_cleanup_sql`, `wpLoad`, `wpExpand`, `forgetPassword` wrappers (and `wp_transient_cleanup_sql`, `options::cleanup_sql` in Rust).
- Tests: `Plans` (unknown id → `invalid_input`, atomic take, undo order, cap); each `plan_*` over `FakeDatabase` (empty-string undo, binary → no undo, key-column change, `DuplicateRow` non-unique refused); frontend apply/undo by id; PendingTray busy state. Docs: `docs/database.md` "Editing rows", `docs/wordpress.md` "Writes".

**A8 Generated types.** Add `ts-rs` (`serde-compat`); `#[derive(TS)] #[ts(export, export_to = "...")]` on every type that crosses IPC (core → `src/api/types.gen.ts`, later `src/stage/types.gen.ts`, `src/wp/types.gen.ts`); `bun run types` = `cargo test --manifest-path src-tauri/Cargo.toml --lib export_bindings` (ts-rs exports from tests); `src/api/types.ts` shrinks to re-exports plus the few frontend-only types (`WpList` moves to Rust as an enum). Fix the drifts found: `BrowserSection.list`/`InsightAction.list` typed; `InsightAction.kind`, `PeekFact.kind`, `TimelineEvent.level`, `ValueNode.kind`, `FilterKind`, `ChipTarget`, `DayBucket` series, `ColumnInfo.autoIncrement/generated`, `TableStructure.primaryKey`, `DocField.autoload`, `Hint.flag.opposite`, `AppError.reason` for `ConfirmationRequired`, `SessionInfo.account`, `PlannedChange`. A CI check: generated files committed and `git diff --exit-code` after `bun run types` in the `lint` script. Docs: `docs/frontend.md` "API boundary", `docs/backend.md` "Adding a command".

### 2.B Frontend seams

**B1 Store slices.** `Store` becomes an `interface`; `registerSlice(slice)` with `create` and `sessionClosed(sessionId, s) -> Partial`; `resetStore` re-applies slices; `disconnect` in one `set` merging every slice's cleanup; `src/state/events.ts` (`onSessionEvent`, `emitSessionEvent("databaseChanged" | "sessionClosed")`); `state/selectors.ts` (`selectActiveSession`, `selectActiveTab`, `selectVisibleDatabases`, `selectActivePending`); core slices `settings`, `session`, `tabs`, `changes`, `data`, `favorites`, `ui` in `state/slices/*.ts` per the table in Part 1 §4 (a). Tests: disconnect purge through the UI; slice registration and reset.

**B2 Settings registry.** `registry/settingsPages.ts` (`registerSettingsPage({id, group, label, icon, intro, Body, order?})`, `settingsPages()`, `settingsGroups()`); `components/settings/{SettingsDialog,controls,ShortcutsPage,pages}.tsx`; core pages registered in `pages.tsx`; `SETTINGS_PAGES`, `NAV`, `INTRO`, `PAGE_LABEL` gone; `src/stage/state.ts` and `commands.ts` read `settingsPages()`. Tests: a registered page shows up in nav and renders its body; unknown page falls back.

**B3 Scope registry.** `commands/scopes.ts` (`registerScope({id, parent, label, isActive})`, `activeScopes`); `Scope = string`; core registers `global`, `db`, `table`, `editor`; `SCOPE_LABEL` lives here. Tests: specificity by depth with registered scopes.

**B4 Modes and slots.** `registry/modes.ts` (`registerMode({id, label, icon, available, Workspace, showFavoritesBar})`), `registry/slots.ts` (`contribute("status-bar" | "table-row-actions" | "palette-source", item)`); `components/DbWorkspace.tsx` registers `db`; `App`, `Toolbar` (toggle from `modes()`, shown when > 1 available), `StatusBar`, `TableTab`, `CommandPalette` consume registries; `ErrorPanel.tsx` own file; `openInDatabase` / `openRow(table, column, value)` store actions (Rust builds the filter from `BrowseRequest.where: {column, value}[]`; delete `lib/sql.ts::sqlString/whereEquals` and every frontend WHERE builder). WP still registers from a temporary `src/components/wp/install.ts`. Tests: a second mode shows the toggle; row actions from a slot appear in the context menu; `openRow` sends a structured filter.

**B5 Module list and boundary.** `src/modules.ts` (`installModules()`), called in `main.tsx`; `src/test/boundary.test.ts` asserts only `api.ts` files import `@tauri-apps/api`; `window.databee` gated by `VITE_DEV_HOOKS` (set in `scripts/dev/launch.sh`). Docs: `docs/architecture.md` rule 2 and a new "Modules" section; `docs/frontend.md` "Boot", "Store", "API boundary".

### 2.C Stage module

**C1 Rust plugin.** `stage/settings.rs` (`StageSettings`, `sanitized` regenerating an empty token, `StageStore` over `JsonStore` at `stage.json`, one-shot migration from `settings.json.stage`); `stage/commands.rs` (from `stage_commands.rs`: `stage_anchors`, `stage_done`, `stage_info`, `stage_status`, `stage_settings`, `stage_update_settings`, `stage_new_token` via `Stage::rotate_token`); `stage::init(config_dir) -> TauriPlugin<Wry>` with `invoke_handler`, `setup` (manage `Stage`, start), `on_event(Exit → stop)`; `pub const COMMANDS` plus a test that `build.rs` lists them; `build.rs` `.plugin("stage", InlinedPlugin::new().commands(..).default_permission(AllowAllCommands))`; `capabilities/stage.json`; delete `Settings.stage`, `StageSettings`, `STAGE_PORT`, the setup block, the five registrations, the restart in `update_settings`, the settings test. `new_token` from 32 `getrandom` bytes. Docs: `docs/stage.md` "Code", removal recipe; `docs/backend.md` tables.

**C2 Frontend module.** `src/stage/{install,api,apiMock,types.gen,state,commands,hooks,rect}.ts`, `PresentationsPage.tsx` (local state via `stageApi.settings()/updateSettings()`; no store action); `installStage()` installs hooks and registers the settings page; `src/lib/stage.ts` → `src/stage/rect.ts`; remove every stage item from `api/index.ts`, `types.ts`, `test/apiMock.ts`, `test/settings.ts`, `store.ts`, `App.tsx`, `SettingsDialog`. `api.ts` uses `plugin:stage|…`. Tests: existing `hooks.test.tsx` moves and mocks `./api`; PresentationsPage test (status, new token, copy register command) via `createStageApiMock`.

**C3 Stage internals.** Findings Part 1 §2: `hooks.rs` → `hub.rs` (`Hub`, `StageHost`, `Timeouts`, `Waiter`, `ask`, `receive_*`), `windows.rs` (`Frame`, `Window` enum, `WindowInfo`/`AnchorInfo` serializable, `is_shown`), `verbs.rs` (`run`, `describe`, `WindowRequest` parsing, `ActKind` enum, `type_payload`, `scroll_payload`, `wait`, `keys`), `test_host.rs` (`FakePage`); `protocol.rs` `parse_keys` split, one label limit (page side); `http.rs` accept backoff, `response` returns bytes; `StageActRequest` as a discriminated union (`#[serde(tag = "kind")]`) mirrored by ts-rs; `hooks.ts` → `anchors.ts`, `actions.ts`, `hooks.ts`; comma expressions rewritten; every "§n" reference replaced. Tests: the missing cases in Part 1 §2.

### 2.D WordPress module

**D1 Rust plugin.** `wp/commands.rs` (from `wp_commands.rs`; `WpEnv { ctx, catalog, session }`, `OrderType` enum, `CatalogCache` by `(path, mtime)`); `wp/snapshots_store.rs` (from `store/snapshots.rs`); `WpState { snapshots, catalogs }`; `wp::init(config_dir)`; `build.rs` + `capabilities/wp.json`; delete the 30 registrations and `AppState.snapshots`; `tests/wp.rs` and `tests/php_roundtrip.rs` → `wp/tests.rs` (`#[cfg(test)]`, `#[ignore]`, via `crate::`) with the server helpers in `src/test_support.rs` (`test-util`), `tests/common/mod.rs` for `tests/mysql.rs`. Tests: `ChangeRequest::database()`, `OrderType`, catalog error formatting, `CatalogCache` invalidation by mtime. Docs: `docs/wordpress.md` header and "Tests", `docs/backend.md`.

**D2 Frontend module.** `git mv src/components/wp src/wp/components`; `src/lib/crumbs.ts` → `src/wp/routes.ts` (with `LIST_LABELS`), `wpFormat.ts` → `src/wp/format.ts`; `src/wp/{install,api,apiMock,types.gen,state,commands}.ts`, `wp.css` (the `wp` sections of `app.css`), `src/wp/test/fixtures.ts` (from `test/wpFixtures.ts`); `wpSlice` (`wp`, `wpNav`, `entityMeta`, `loadWp`, `navigate`, `openEntity`, `queueWpChange`, `takeSnapshot`); `installWordPress()` registers slice, scopes `wp`/`document`, commands (`go.*`, `view.wp`, `nav.*`, `wp.snapshot`, `document.source`), mode, settings page, palette source, status-bar and table-row-action contributions, `databaseChanged` listener; `recentNav` generalized in core to `{key, title, badge, go}`; core `App`, `Toolbar`, `StatusBar`, `TableTab`, `CommandPalette`, `commands/index.ts`, `store.ts`, `api/*`, `test/apiMock.ts` lose every WP reference. Tests: `WpMode.test.tsx` moves and mocks both api modules; a core `App.test` without `installWordPress()` shows no mode toggle. Docs: `docs/wordpress.md` "Screens", removal recipe; `docs/adapting.md` shrinks "Removing the WordPress layer" to the recipe.

**D3 WP internals.** Apply the layout from Part 1 §3 (a) inside `src-tauri/src/wp/`: `sql.rs` (`Filter` builder, `like_contains`/`like_prefix`, `PAID_STATUSES`, `AUTOLOAD_ON`, `table_ref`), `rows.rs` (one `text`/`num`/`int`/`cell_str`, `column_index`, `facets`), `context/{mod,probe,timezone}.rs`, `hints/{mod,rules,sniff}.rs` (compiled globs, `guessed: true`), `entity/{def,sql,assemble,load}.rs` (`MetaDef.id`, `currency`/`image`/`peek` keys in `entities.toml`, user overrides by name, `hidden` removed), `lists/{mod,orders,users,products,actions,sessions,content}.rs` (`ListKind`, `default_desc`, `customer` filter, `EXISTS` instead of `HAVING`, facets for trash/internal types, `CartSummary`, `guest: bool`), `views/{mod,common,timeline,order,order_items,payment,customer,guest,product,action,session}.rs` (`TimelineEvent` enums with `RowRef`, `Totals` with source keys, `PaymentPanel` from TOML, `ChipTarget` enum, `GuestView` without a fake doc, variation rendered itself), `search.rs` (`SearchSource` table, concurrent queries, min length for meta scans), `peek.rs` (facts from TOML, `key` on `PeekFact`), `find.rs` (sources from the catalog, `LIMIT n+1`, core tables), `dashboard/{mod,orders,actions,tables,insights,browser}.rs` (enums, `fold_status` with `cancelled`, concurrent counts, core-first `table_category`, site-timezone days, drift by compared columns), `options.rs` (`OptionGroup` enum, prefix-correct SQL, `total`/`truncated`, orphan timeouts), `snapshot.rs` (escaped paths, typed leaves, `__truncated`), `changes/{mod,meta,woo,options}.rs` (per-`meta_id` plans, sync mirroring or warning, exact status undo, `scheduled_date_local`), `logs.rs` (`ENDPOINTS` table), `media.rs` (URL encoding). Every [product] item in Part 1 §3: raw keys next to labels, nothing hidden (collapsed at most), no naming heuristics without `guessed`. Frontend counterpart in `src/wp/`: `Lists` split (`lists/config.ts`, `cells.tsx`, `ListToolbar`, `ListTable`, `DetailPane`, `useListQuery`), `FieldValue` split (pivot and flag opposite from Rust), `OrderView` (tax lines shown, `[sourceKey, value]` rows, statuses from Rust, logs via `useWpData`, `KvRow`), `OtherViews` one file per view (`OrderRow`, allowed actions from Rust), `Dashboard` split (raw table names, `hposLabel`, `autoloadHealthy` flag), `Debugging` (`DiffTable`), `common.tsx` keys, `EntityScreen` (ticking "updated", cleared `changed`, validated Compare), `Settings.tsx` (`autoload` field, `data.table`), `WpWorkspace` (`extra` on nav groups, severity from Rust). Tests: Part 1 §3 per-module lists; exact-SQL tests replacing `contains`; `tests` integration gaps (apply every variant, posts copy, NULL/empty undo, exact drift/peek/find assertions, own rows per write test). Docs: `docs/wordpress.md` modules table and "Extending". This step is large: commit per module folder (`sql`+`rows`, `context`, `hints`, `entity`, `lists`, `views`, `dashboard`, `options`+`snapshot`, `changes`, `search`+`peek`+`find`, frontend per component).

---

## Phase 3: performance and tests

- **3.1 Stores off the async path**: `JsonStore` writes through `spawn_blocking` (or one background writer with coalescing); Keychain calls in `session::open` through `spawn_blocking`; `history.record` debounced; snapshots as one file per snapshot plus an index.
- **3.2 Query path**: `LONGTEXT` truncated in Rust to `{len, preview}` with a `fetch_cell` command for the full value (`Inspector` and `ResultsGrid` use it); `Family` once per result set; hex without per-byte allocation; `browse`/`fetch` timeouts `KILL QUERY` the pooled connection id; partial results plus failing statement index on mid-script errors; `session_reset` flag after a timeout reconnect (UI toast).
- **3.3 WordPress query counts**: batched titles, no images for unrendered links, row+meta peek, `actions_referencing(id)` with `JSON_CONTAINS`, `try_join!` for browser counts and search, `LIMIT n+1` find, order logs without `load_docs`, `entity_view` passing the loaded doc.
- **3.4 Frontend rendering**: `memo(QueryTab)`, `memo(TableTab)`; `Sidebar` selects the active table key only; `StatusBar`/`Toolbar`/`ResultsGrid` narrow selectors; `useWindowSize` via `useSyncExternalStore`; horizontal virtualization in `ResultsGrid`; virtualized `HistoryPanel` and transient list; `ValueTree` auto-open capped; `LongText` byte size from Rust.
- **3.5 Tests**: every remaining "[test] missing" bullet in Part 1 not yet covered; `tests/common/mod.rs`; `cancel_interrupts_a_running_query` polls `PROCESSLIST`; `timeout_aborts_and_session_recovers` runs `USE mysql` first; `php_roundtrip` reads `HEX(col)`, pages, reports skipped; `wordpress_mode_queries_work_on_real_data` panics when `DATABEE_WP_TEST` is unset under `--ignored`; integration cases listed under tests/mysql.rs.

## Phase 4: comments, CSS, tooling

- **4.1 Comments**: run `clean-up-code` over every file; delete the narrating and stale comments quoted in Part 1 (`[comment]` items), every `§n` reference, every em dash in code, comments and strings; `ui_state` field renames; inline enumerations on struct fields replaced by enums (done in Phase 2 where listed).
- **4.2 CSS**: delete the dead selectors and `--tile-*` tokens; merge the duplicate rules; one dark-token block; tokens `--sidebar-width`, `--wp-nav-width`, `--radius-pill`, `--hairline`, `--on-accent`, `--shadow-control`, `--scrim`; status colors through one role; rem constants in `ResultsGrid` and `theme.ts`; `SearchInput` component; no inline px styles; shortcut labels from `bindingLabel(bindingFor(id))` everywhere.
- **4.3 Tooling**: ESLint flat config with `typescript-eslint` and `react-hooks` (`exhaustive-deps` as error; fix the four effects instead of disabling); `noUncheckedIndexedAccess: true`; vitest `restoreMocks: true`, `unstubGlobals: true`; `bun run lint` runs eslint and the generated-types diff check; `docs/testing.md` updated.
- **4.4 Docs**: final pass over `docs/*.md` and `README.md` against the new layout; `docs/architecture.md` repository map; removal and swap recipes in `docs/adapting.md`.

---

## Architecture Decision Records

`docs/adr/NNNN-<slug>.md`, numbered in order of acceptance, indexed from `docs/adr/README.md` and linked from `docs/README.md`. Format (Nygard): `# NNNN. Title`, `Status` (Proposed / Accepted / Superseded by NNNN), `Context` (the forces, in a paragraph), `Decision` (one or two paragraphs, what we do and why this over the alternatives), `Consequences` (good and bad, what becomes easier and harder). Nothing else: no task lists, no file layouts beyond what the decision needs, no API specs. An ADR records why; the docs pages record how. Each is at most a page.

Rules for the implementer:
- Write the ADR in the same commit as the step that makes the decision, with status Accepted.
- Only decisions that constrain future work and that someone could reasonably have made differently get a record. A refactor, a bug fix, or a naming choice does not.
- When a later step reverses a decision, write a new ADR and mark the old one Superseded; never edit an accepted ADR's decision text.

Retroactive records (written in step B5, status Accepted, dated by the commit that introduced the decision; they capture choices the code already embodies):
- 0001 Rust owns the logic, React only renders
- 0002 Single IPC boundary in `api.ts` files (updated in B5 from "one file" to "per module")
- 0003 Errors cross IPC as `{kind, message, code}`
- 0004 Passwords live in `Secret`, never in files, logs or the webview
- 0005 Custom Unix-socket single-instance handoff instead of `tauri-plugin-single-instance` (passwords stay out of argv)
- 0006 Two channels per connection: a dedicated editor connection plus a metadata pool
- 0007 Plan → review → apply in one transaction → undo for every write
- 0008 WordPress layer shows stored data, no naming heuristics (the `wp-layer.md` rules)
- 0009 Whole script sent as one multi-statement call, results split by the lexer's count

Records made by this plan, written in the step named:
- 0010 Optional features are inline Tauri plugins with one install line (A1/C1)
- 0011 Driver seam is a `Database` trait object with a `Dialect` value, not an enum (A3)
- 0012 Planned changes are stored server-side and applied by id (A7)
- 0013 Per-session module data lives in `Session.ext` (A5)
- 0014 TypeScript IPC types are generated from Rust with ts-rs (A8)
- 0015 Frontend modules contribute through registries (slices, scopes, modes, settings pages, slots); the core never imports a module (B1–B5)
- 0016 Module settings live in the module's own JSON file, not in `settings.json` (C1)
- 0017 Keep `panic = "unwind"` so a panic in one command does not take the app down (1.3)
- 0018 Value escaping follows the server's `sql_mode` (`Dialect.backslash_escapes`) (A3)
- 0019 `MemoryStore` and `FakeDatabase` ship behind a `test-util` feature (A2/A3)

Decisions explicitly not recorded as ADRs (they are implementation choices, go in the docs): file splits, store slicing details, naming, test conventions, CSS tokens.

## Target layout after Phase 2

```
src-tauri/
  build.rs                    one .plugin(..) line per module
  capabilities/               default.json, stage.json, wp.json
  src/
    lib.rs                    manage(AppState) → .plugin(stage::init) .plugin(wp::init) → setup → core generate_handler!
    cli.rs ipc.rs launch.rs menu.rs window.rs dev.rs error.rs connection.rs state.rs session.rs commands.rs db_commands.rs
    db/     mod.rs (Database trait, open) types.rs dialect.rs browse.rs lexer.rs safety.rs cell.rs id_filter.rs changes.rs plans.rs fake.rs mysql/{mod,editor,introspect,script}.rs
    decode/ php.rs value.rs edit.rs commands.rs
    secrets/ mod.rs keychain.rs memory.rs
    store/  json_file.rs json_store.rs settings.rs keybindings.rs recents.rs favorites.rs history.rs ui_state.rs
    stage/  mod.rs settings.rs commands.rs server.rs hub.rs windows.rs verbs.rs http.rs protocol.rs test_host.rs
    wp/     mod.rs commands.rs cache.rs snapshots_store.rs tests.rs sql.rs rows.rs context/ hints/ entity/ lists/ views/ search.rs peek.rs find.rs dashboard/ options.rs snapshot.rs changes/ logs.rs media.rs entities.toml meta_keys.toml
  tests/  common/mod.rs mysql.rs
src/
  main.tsx modules.ts App.tsx
  api/        index.ts types.gen.ts types.ts
  state/      store.ts selectors.ts events.ts slices/
  commands/   registry.ts scopes.ts index.ts
  registry/   settingsPages.ts modes.ts slots.ts
  components/ (core UI) settings/ DbWorkspace.tsx ErrorPanel.tsx …
  lib/        format.ts sql.ts theme.ts
  styles/     theme.css app.css
  test/       apiMock.ts fixtures.ts settings.ts boundary.test.ts
  stage/      install.ts api.ts apiMock.ts types.gen.ts state.ts commands.ts hooks.ts anchors.ts actions.ts rect.ts PresentationsPage.tsx
  wp/         install.ts api.ts apiMock.ts types.gen.ts state.ts commands.ts routes.ts format.ts wp.css components/ test/
```

## Removal and swap recipes (go into `docs/adapting.md`)

**Remove Stage**: `git rm -r src-tauri/src/stage src/stage src-tauri/capabilities/stage.json docs/stage.md`; delete `pub mod stage;` and `.plugin(stage::init(&config_dir))` in `lib.rs`; delete the `.plugin("stage", …)` line and `STAGE_COMMANDS` in `build.rs`; delete the `installStage` import and call in `src/modules.ts`; optionally strip `data-stage*` attributes; run `bun run lint && bun run test`.

**Remove WordPress**: `git rm -r src-tauri/src/wp src/wp src-tauri/capabilities/wp.json docs/wordpress.md .claude/rules/wp-layer.md`; delete the two `lib.rs` lines, the `build.rs` line and `WP_COMMANDS`, the `modules.ts` lines; delete the `wp_test`/`wp_write`/`wp_legacy` sections of `dev/fixture.sql`; run tests. The Inspector keeps decoding (core `decode/`); grid edits keep working (core `db/changes.rs`); the Toolbar shows no mode toggle.

**Swap the secret store**: add `src-tauri/src/secrets/<impl>.rs` implementing `SecretStore` (`get`, `set`, `delete`); `pub mod` it in `secrets/mod.rs`; change the one `Box::new(secrets::Keychain)` line in `lib.rs`; optionally delete `keychain.rs` and `security-framework`.

**Add a driver**: `src-tauri/src/db/<engine>/mod.rs` implementing `Database`; an `Engine` variant with its `Dialect`; a `kind` on `ConnectionSpec`; one arm in `db::open`. Nothing in `wp/` changes.

## Dependency summary

1.3 → A3 → {A4, A5, A7, D1}; A7 → {D1, D2}; A6 → D1; A1 → C1; 1.2 → {C1, D1}; B2 → C2; B1+B3+B4+B5 → D2; C1 → C3; D1 → D3; A8 can start after A7 and must finish before C2/D2 (they ship generated types). Phase 1 steps are independent of each other. Phase B and C1 can run in parallel with A4–A7 (disjoint files).

## Verification (end of each phase)

1. `ci` agent: UI tests, Rust unit tests, lint, format check; after Phase 1.6 and D3 also `bun run test:db` (the Docker fixtures, including `wp_legacy`).
2. `bun run build:app`, then `scripts/dev/launch.sh mysql://root:root@127.0.0.1:33306/wp_test`; drive with `scripts/dev/js.sh` and capture with `scripts/dev/screenshot.sh`: connect, open a table, edit a cell, apply, undo; open WordPress mode, an order, the dashboard; open Settings → Presentations; register with the local Stage and run the `docs/stage.md` curl script (`describe`, `window`, `act` type/keys).
3. After Phase 2: apply both removal recipes on a scratch branch and confirm `bun run lint && bun run test` pass with the module gone; revert.
4. After A8: `bun run types` produces no diff.
