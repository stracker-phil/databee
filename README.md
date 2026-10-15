# databee

A minimalistic SQL client for macOS (Apple Silicon), built for MySQL/MariaDB in
WordPress & WooCommerce development. Tauri 2 + Rust + React.

## Features

- **CLI-first launching:** another app or script opens databee with connection details. A second launch hands its request to the running window.
- **Keychain:** passwords are stored in the macOS Keychain and never written to files or logs.
- **Browsing:**
  - schema tree with filter (⌘K)
  - paged table view with sorting and a `WHERE` filter
  - structure view (columns, indexes, DDL)
- **SQL editor** (CodeMirror):
  - MySQL autocomplete
  - ⌘↵ runs the statement at the cursor, ⇧⌘↵ runs everything
  - cancel, timeout, row limit
  - asks before `DROP`/`TRUNCATE`/unscoped `DELETE`/`UPDATE`
- **Results grid:** virtualized, typed rendering. Copy cells or rows as TSV/JSON.
- **Value inspector:** decodes **PHP-serialized** and **JSON** values into a tree, including serialized data nested inside strings. Values with broken string lengths are decoded leniently, with a warning.
- **Favorites bar** (⌘D) for tables and saved queries, and **query history** per server.
- **WordPress lens** (when a WP database is detected):
  - detects the table prefix, multisite, WooCommerce version and HPOS mode (off / on / with sync)
  - entity documents: posts, users, comments, orders (HPOS or legacy), order items, addresses, payment tokens. Meta is split into public and private and grouped by prefix. Values are decoded, IDs link to other documents, and relations load lazily.
  - options inspector: autoload weight per plugin prefix, transients with expiry, cleanup SQL to review
- **Settings:** theme, accent, density, row limit, timeouts, NULL label, log level, and more.

## CLI

```sh
databee mysql://user:pass@127.0.0.1:3306/wordpress
databee -H 127.0.0.1 -P 52958 -u db -d db --name "Shop (DDEV)" --password-env DB_PASS
echo "$PASS" | databee -u root -d local --socket ~/Library/.../mysqld.sock --password-stdin --save-password
databee --help
```

Password sources, in order: `--password` / `--password-stdin` / `--password-env`, then the
URL, then the Keychain (account `user@host:port`), then a prompt in the app.

To install the `databee` command, run `scripts/install-cli.sh`. It links `~/.local/bin/databee` to the built app.

## Development

Requires rustup (stable), bun and Xcode. Docker is needed for the database tests.

| Command | What it does |
|---|---|
| `bun install` | install JS dependencies |
| `bun run app` | debug build of the `.app` bundle and launch it |
| `bun run test` | Vitest UI tests and Rust unit tests |
| `bun run test:db` | start the MariaDB 11.4 + MySQL 8.4 fixtures (`dev/`) and run the integration tests |
| `bun run lint` | `tsc` and `clippy -D warnings` |

The round-trip check against a real WordPress DB (read-only) runs with:
`DATABEE_WP_TEST=mysql://root:root@127.0.0.1:52958/db cargo test --manifest-path src-tauri/Cargo.toml --test php_roundtrip -- --ignored --nocapture`

**Files:** config and data live in `~/Library/Application Support/com.philippstracker.databee/`
(`settings.json`, `favorites.json`, `connections.json`, `history.json`, and the optional
`entities.toml` for custom WordPress entities). Logs are in `~/Library/Logs/com.philippstracker.databee/`.

### Custom entities

Add `entities.toml` to the config directory, using the format of
`src-tauri/src/wp/entities.toml`. A definition with the same `name` and `when` replaces a built-in one.
```toml
[[entity]]
name = "vault_token"
label = "PayPal vault token"
table = "ppcp_vault_tokens"     # without prefix
id = "id"
title = "customer_id"
links = { user_id = "user" }
```
