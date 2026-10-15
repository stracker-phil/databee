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
- **WordPress mode** (the default for WordPress databases; switch to **Database** mode for tables and SQL any time):
  - **Dashboard:** orders, autoload weight, failed actions and expired transients at a glance; a table-size treemap; 14-day order bars; Action Scheduler health; recent orders; insights such as "the log table holds 54% of all rows".
  - **Browse:** orders, subscriptions, customers, products, scheduled actions, sessions and content, each with status chips, filters, search and counts in the sidebar.
  - **Purpose-built pages:**
    - Orders: addresses side by side with mismatch hints, line items with thumbnails, totals, a PayPal panel with a fee waterfall, attribution, and a timeline merging notes, actions, logs, refunds and the shopping session.
    - Customers: roles, addresses, orders, subscriptions and saved cards.
    - Products: variation matrix.
    - Actions: arguments and log.
    - Sessions: decoded cart.
  - **Readable values:** money, relative times, flags, links, user agents, countries, roles and guest badges; secrets stay masked; long values collapse; meta is grouped by a key registry (Profile, Billing, PayPal, Attribution, Admin UI, …).
  - **⌘K omnisearch:** paste an order ID, email, PayPal order or transaction ID, or SKU to jump straight to the record. A relation strip, breadcrumbs and ⌘[ / ⌘] handle navigation.
  - **Editing without silent writes:** inline field edits, serialized values edited as a tree (re-serialized with correct byte lengths), order status, autoload toggles, retrying or cancelling actions, transient cleanup, and grid editing in Database mode. Every change goes into a pending tray with its SQL, applies in one transaction (⌘S) and can be undone.
  - **Debugging:** live mode highlights fields as they change; snapshot an order (with related records and actions) or an options pattern, then diff it; compare two entities side by side.
  - **Screen-share mode** blurs names, emails and addresses.
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
