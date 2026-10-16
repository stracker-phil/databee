# Testing and dev tooling

## Commands

Coding agents don't run these themselves. They hand test writing to the `write-ts-unit-test` and
`write-rust-unit-test` agents and runs to the `ci` agent (`.claude/agents/`), so test code and test output stay
out of the main context ([`.claude/rules/testing.md`](../.claude/rules/testing.md)).

| Command | Runs |
|---|---|
| `bun run test` | Vitest UI tests (`test:ui`), then the Rust unit tests (`test:rust`) |
| `bun run test:db` | recreates the Docker fixtures, then `cargo test -- --ignored` (integration tests only, in `tests/` and in modules) |
| `bun run lint` | `tsc --noEmit` and `cargo clippy --all-targets -- -D warnings` |
| `cargo fmt --manifest-path src-tauri/Cargo.toml` | Rust formatting (run before lint) |

The project rules for tests are in [`.claude/rules/testing.md`](../.claude/rules/testing.md):
- Every change ships with tests of its observable behavior.
- Tests go through public interfaces: command results, CLI parse results, rendered UI.
- Internals and incidental side effects are never tested.

## Rust

- **Unit tests** sit next to the code in `#[cfg(test)] mod tests`. Stores use `tempfile` directories, and
  secrets use `MemoryStore`. Nothing touches the real config dir or the Keychain. Pure SQL builders are
  tested by exact string.
- **Integration tests** need a server, so every one is `#[ignore]`. The core's live in `src-tauri/tests/` and use
  the public `databee_lib` API; a module's live inside the module, so they are deleted with it. The fixture
  helpers (`servers`, `spec`, `connect_to`, `sessions`) are in `src/test_support.rs`, compiled with `cfg(test)`
  or the `test-util` feature; `tests/common/mod.rs` re-exports them for the test binaries.

  | File | Needs | Run alone |
  |---|---|---|
  | `tests/mysql.rs` | the Docker fixtures; checks the driver on MariaDB and MySQL | `cargo test --test mysql -- --ignored` |
  | `tests/sql_mode.rs` | the Docker fixtures (`wp_write`); changes the global `sql_mode`, so it is its own binary | `cargo test --test sql_mode -- --ignored` |
  | `src/wp/server_tests.rs` | the Docker fixtures (`wp_test` and `wp_legacy` read-only, `wp_write` for edits); two tests read `DATABEE_WP_TEST=mysql://user:pass@host:port/db`, a real WordPress database of your own, read-only, and skip when it is unset | `cargo test --lib -- --ignored wp::` |

## Docker fixtures

`dev/docker-compose.yml` (project `databee-fixtures`) runs two servers:
- MariaDB 11.4 on `127.0.0.1:33306`
- MySQL 8.4 on `127.0.0.1:33307`

Both use root password `root` and are seeded from `dev/fixture.sql`. `test:db` recreates the containers on
every run, because the fixture uses `UTC_TIMESTAMP()` for relative dates.

`DATABEE_TEST_SERVERS="host:port,host:port"` points the tests at other servers.

| Database | Contents |
|---|---|
| `databee_test` | not WordPress: posts-like table, one column per decoder family, a view |
| `wp_test` | WordPress plus WooCommerce (HPOS with sync) schema and seed data for the lens tests |
| `wp_write` | `CREATE TABLE … LIKE` copies for edit and undo tests |
| `wp_legacy` | WooCommerce with HPOS off: orders in `wp_posts` (a customer with two orders, a guest order, a refund, order notes), no `wc_orders` tables |

The fixtures are also useful for manual testing:

```sh
docker compose -f dev/docker-compose.yml up -d --wait
databee mysql://root:root@127.0.0.1:33306/wp_test
```

## Frontend

The setup, mocking and fixtures are described in [frontend.md](frontend.md#tests). In short:
- Tests are `*.test.tsx` files next to their components.
- They mock the `src/api` module with `createApiMock()`.
- They use Testing Library queries by role or text.

## Driving the app

The dev scripts work without stealing focus, so the app can run on a second screen while you or an agent
keep typing in the terminal. They are macOS-only (zsh, Swift, `screencapture`).

| Script | Does |
|---|---|
| `bun run build:app` | builds the debug `.app` bundle |
| `scripts/dev/launch.sh [cli args]` | kills a running DataBee and starts the debug build next to the terminal window, without focus. The log goes to `$TMPDIR/databee-dev.log`. The terminal app is set by `DATABEE_DEV_TERMINAL` (default `Terminal`). |
| `scripts/dev/js.sh '<js>'` | runs JavaScript in the running debug app over the IPC socket. It is fire-and-forget: there is no return value, and top-level `const` declarations collide across calls, so wrap them in an IIFE. |
| `scripts/dev/screenshot.sh out.png` | captures the DataBee window, also while it is in the background |
| `bun run app` | builds and `open`s the app; this takes focus |

The store is available as `window.databee` in the debug bundle: `bun run build:app` (and `bun run app`) set
`VITE_DEV_HOOKS=1` at build time, and `main.tsx` exposes the store only then. A release build (`bun run build`)
has no `window.databee`.

```sh
scripts/dev/js.sh 'databee.getState().openDialog({type:"settings", page:"shortcuts"})'
scripts/dev/js.sh '(() => { const s = databee.getState(); s.setMode("db"); s.openTableTab("wp_test", "wp_options"); })()'
```

`launch.sh` sets `DATABEE_WINDOW_FRAME` and `DATABEE_BACKGROUND=1`. Rust reads both in `dev.rs`, a module
compiled into debug builds only. JavaScript eval over the socket is compiled out of release builds as well.
