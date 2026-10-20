# Testing and dev tooling

## Commands

Coding agents don't run these themselves. They hand test writing to the `write-ts-unit-test` and
`write-rust-unit-test` agents and runs to the `ci` agent (`.claude/agents/`), so test code and test output stay
out of the main context ([`.claude/rules/testing.md`](../.claude/rules/testing.md)).

| Command | Runs |
|---|---|
| `bun run test` | Vitest UI tests (`test:ui`), then the Rust unit tests (`test:rust`) |
| `bun run test:db` | recreates the Docker fixtures, then `cargo test -- --ignored --skip real_data` (integration tests only, in `tests/` and in modules; the real-data tests are left out) |
| `bun run lint` | regenerates the IPC types (`types`: the ts-rs `export_bindings` tests) and fails when `src/api/types.gen.ts` or `src/stage/types.gen.ts` changed, then `tsc --noEmit` (app and tests), `eslint .` (`eslint.config.js`: `@eslint/js` and type-checked `typescript-eslint` recommended plus `react-hooks/rules-of-hooks` and `exhaustive-deps`, no style rules; test files and `apiMock.ts` turn off the `no-unsafe-*`, `unbound-method` and `require-await` rules that fight mocks) and `cargo clippy --lib --bins -- -D warnings` (production code only; test code is compiled by `cargo test` but not linted, because tests deliberately use `unwrap`, clones and odd inputs) |
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
  helpers (`servers`, `spec`, `connect_to`, `sessions`, and `Counting`, a `Database` wrapper whose
  `StatementLog` lists what reached the server) are in `src/test_support.rs`, compiled with `cfg(test)`
  or the `test-util` feature; `tests/common/mod.rs` re-exports them for the test binaries.

  | File | Needs | Run alone |
  |---|---|---|
  | `tests/mysql.rs` | the Docker fixtures; checks the driver on MariaDB and MySQL | `cargo test --test mysql -- --ignored` |
  | `tests/changes.rs` | the Docker fixtures; row edits planned by the core (`db::changes`) on `wp_write` and `databee_test`: serialized edits, NULL and `''` restored by undo, delete, duplicate and insert, a key column change, no undo for binary cells; every test owns its rows (ids from 9100 up) | `cargo test --test changes -- --ignored` |
  | `tests/sql_mode.rs` | the Docker fixtures; plans, applies and undoes a `SetCell` on a meta row of `wp_write` (`wp_postmeta` 9303) while the server's global `sql_mode` includes `NO_BACKSLASH_ESCAPES` (restored afterwards; its own binary so nothing else connects meanwhile) | `cargo test --test sql_mode -- --ignored` |
  | `tests/health.rs` | the Docker fixtures (`databee_test` read-only, `wp_write` as the session's database); a killed editor connection is replaced with its session state restored, and a server that stops answering (reached through a local TCP relay the test closes) fails `ping` and `reconnect` with a connection error | `cargo test --test health -- --ignored` |
  | `src/document/server_tests.rs` (and the other `server_tests.rs` of `rules/`, `document/actions/`, `search/`, `readout/`) | the Docker fixtures (`wp_test`, `wp_legacy`, `wp_custom`, `wp_multi` and `databee_test` read-only, `wp_write` for actions); the test in `mod real_data` reads `DATABEE_WP_TEST=mysql://user:pass@host:port/db`, a real database of your own, read-only, and round-trips every serialized value of its `[[meta]]` tables | `cargo test --lib -- --ignored document::` |
  | inline `#[ignore]` tests in `table_check.rs`, `session.rs`, `schema.rs`, `value_counts.rs`, `address/resolve.rs`, `rules/annotate.rs`, `rules/coverage.rs`, `rules/lens.rs`, `rules/effective.rs` (their `mod tests`) and `mcp/data.rs` (`mod server_tests`) | the Docker fixtures | `cargo test --lib -- --ignored <path filter>`, e.g. `table_check::` |
  | `src/statement_counts.rs` | the Docker fixtures (`wp_test`, read-only); asserts the statements per `document`, `peek`, `palette_search`, `readout` and `reference_coverage` ([database.md](database.md#statement-counts)) | `cargo test --lib -- --ignored statement_counts` |

  The `real_data` test panics when `DATABEE_WP_TEST` is unset. `test:db` never runs them: its `cargo test` call
  ends in `--skip real_data` (a name filter on the module path), so CI without the variable stays green. Run them
  by name against a site you own:

  ```sh
  DATABEE_WP_TEST=mysql://root:root@127.0.0.1:52958/db cargo test --manifest-path src-tauri/Cargo.toml --lib -- --ignored --nocapture real_data
  ```

## Docker fixtures

`dev/docker-compose.yml` (project `databee-fixtures`) runs two servers:
- MariaDB 11.4 on `127.0.0.1:33306`
- MySQL 8.4 on `127.0.0.1:33307`

Both use root password `root` and are seeded from `dev/fixture.sql`. `test:db` recreates the containers on
every run, because the fixture uses `UTC_TIMESTAMP()` for relative dates.

`DATABEE_TEST_SERVERS="host:port,host:port"` points the tests at other servers.

| Database | Contents |
|---|---|
| `databee_test` | not WordPress: posts-like table, one column per decoder family, a view, `authors`/`books` with a real foreign key (`books_author`) |
| `wp_test` | WordPress plus WooCommerce (HPOS with sync) schema and seed data for the preset, document, palette and readout tests |
| `wp_write` | `CREATE TABLE … LIKE` copies for edit and undo tests, plus rows (ids from 9100 up: orders 9101 to 9403, actions 9201 and 9202, postmeta 9301 to 9303, user 9501, post 9601, address 9602, `zz_*` options and `zz-retention` log entries) that single write tests change and put back; post 9701 with 5,000 meta keys (`zz_long_0001` to `zz_long_5000`, meta ids 100001 to 105000) for the long document group and Rows grid |
| `wp_legacy` | WooCommerce with HPOS off: orders in `wp_posts` (a customer with two orders, a guest order, a refund, order notes), no `wc_orders` tables |
| `wp_custom` | WordPress with table prefix `xy_` (core tables only), for prefix detection |
| `wp_multi` | multisite network: main-site `wp_` tables, `wp_blogs`, `wp_sitemeta` (`active_sitewide_plugins`), sub-site 2 (`wp_2_options`, `wp_2_posts`, no `wp_2_users`) |

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
keep typing in the terminal. They are macOS-only (zsh, Swift, `screencapture`, and `python3` in `js.sh`).

| Script | Does |
|---|---|
| `bun run build:app` | builds the debug `.app` bundle |
| `scripts/dev/launch.sh [cli args]` | kills a running DataBee and starts the debug build next to the terminal window, without focus. The log goes to `$TMPDIR/databee-dev.log`. The terminal app is set by `DATABEE_DEV_TERMINAL` (default `Terminal`). |
| `scripts/dev/js.sh '<js>'` | runs JavaScript in the running debug app over the IPC socket. It is fire-and-forget: there is no return value, and top-level `const` declarations collide across calls, so wrap them in an IIFE. |
| `scripts/dev/screenshot.sh out.png` | captures the DataBee window, also while it is in the background |
| `bun run app` | builds and `open`s the app; this takes focus |

The store is available as `window.databee` in the debug bundle: `bun run build:app` (and `bun run app`) set
`VITE_DEV_HOOKS=1` at build time, and `main.tsx` exposes the store only then. A release build
(`bun run tauri build`) has no `window.databee`; `bun run build` builds the frontend only.

```sh
scripts/dev/js.sh 'databee.getState().openDialog({type:"settings", page:"shortcuts"})'
scripts/dev/js.sh 'databee.getState().openTableTab("wp_test", "wp_options")'
```

`launch.sh` sets `DATABEE_WINDOW_FRAME` and `DATABEE_BACKGROUND=1`. Rust reads both in `dev.rs`, a module
compiled into debug builds only. JavaScript eval over the socket is compiled out of release builds as well.
