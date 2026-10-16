# databee

A minimalistic SQL client for macOS (Apple Silicon), built for MySQL/MariaDB in
WordPress & WooCommerce development. Tauri 2 + Rust + React.

## Features

- **CLI-first launching:** another app or script opens databee with connection details. A second launch hands its request to the running window.
- **Keychain:** passwords are stored in the macOS Keychain and never written to files or logs.
- **Browsing:**
  - schema tree with filter (⌘⇧F); a single click opens a table as a preview tab, double-click or ⌘-click keeps it
  - paged table view with sorting, resizable columns (widths are remembered per connection and table) and a `WHERE` filter (⌘F) that also takes IDs: `3229`, `3229, 3227`, `3220-3229`
  - structure view (columns, indexes, DDL)
- **SQL editor** (CodeMirror):
  - MySQL autocomplete
  - ⌘↵ runs the statement at the cursor, ⇧⌘↵ runs everything
  - cancel, timeout, row limit
  - asks before `DROP`/`TRUNCATE`/unscoped `DELETE`/`UPDATE`
- **Results grid:** virtualized, typed rendering, inline cell editing. Copy cells or rows as TSV/JSON.
- **Value inspector:** decodes **PHP-serialized** and **JSON** values into a tree, including serialized data nested inside strings. Values with broken string lengths are decoded leniently, with a warning.
- **Favorites bar** (⌘D) for tables and saved queries, and **query history** per server.
- **WordPress mode** (the default for WordPress databases; switch to **Database** mode for tables and SQL any time):
  - **Dashboard:** orders, autoload weight, failed actions and expired transients at a glance; a donut of autoload by plugin or table rows; order bars (hourly when only a few days have orders); failed actions grouped by hook; recent orders; alerts with one action each (set a log retention, clean up transients, review failures).
  - **Browse:** orders, subscriptions, customers (including guest checkouts grouped by email), products, scheduled actions, sessions and content. Lists have sortable headers, status chips, filters and search; on wide windows a row opens in a side pane (↑/↓ moves, ↵ opens the page). The sidebar shows counts and only flags what needs attention (failed, on hold, expired).
  - **Purpose-built pages:**
    - Orders: addresses side by side with mismatch hints, line items with thumbnails, totals, a PayPal panel with a fee waterfall, attribution, and a timeline rail merging notes, actions, logs, refunds and the shopping session. PayPal log lines are summarized ("PayPal API · capture order → 201") with the raw entry and its source row one click away.
    - Customers: roles, addresses, orders, subscriptions and saved cards.
    - Products: variation matrix.
    - Actions: arguments and log.
    - Sessions: decoded cart.
  - **Readable values:** money, relative times, flags, links, user agents, countries, roles and guest badges; secrets stay masked; long values collapse; meta is grouped by a key registry (Profile, Billing, PayPal, Attribution, Admin UI, …).
  - **⌘K command palette** (both modes): commands with their shortcuts, tables, entities (paste an order ID, email, PayPal order or transaction ID, or SKU) and recent items. Prefixes: `>` commands, `t:` tables, `#` IDs, `@` emails. Breadcrumbs show where you are; ⌘[ / ⌘] move through history.
  - **Exploration:** hover a customer, product, order or action chip for a peek card; every value has a menu to copy it, filter the matching list by it, or find it everywhere (options, all meta tables, action args, the WooCommerce log).
  - **Plugins lens:** which plugin owns which options, transients, meta keys, hooks, log sources and tables (registry → prefix clustering → gateway values → your manual assignments); storage per plugin; leftover data of inactive plugins with a reviewed cleanup.
  - **Editing without silent writes:** inline field edits, serialized values edited as a tree (re-serialized with correct byte lengths), order status, autoload toggles, retrying or cancelling actions, transient cleanup, and grid editing in Database mode. Every change goes into a pending tray with its SQL, applies in one transaction (⌘S) and can be undone.
  - **Debugging:** live mode highlights fields as they change; snapshot an order (with related records and actions) or an options pattern, then diff it; compare two entities side by side.
- **Settings:** theme, accent, UI and data text sizes (⌘= / ⌘- / ⌘0 zoom both), density, row limit, timeouts, NULL label, log level, and every shortcut (rebind or unbind; conflicts are flagged).

## Shortcuts

| Shortcut | Command | Where |
|---|---|---|
| ⌘K | Command palette | everywhere |
| ⌘P | Open table or entity | everywhere |
| ⌘1 / ⌘2 | WordPress / Database view | everywhere |
| ⌘[ / ⌘] | Back / forward | WordPress |
| ⌘= / ⌘- / ⌘0 | Zoom in / out / reset | everywhere |
| ⌘R | Refresh current view | everywhere |
| ⌘F | WHERE filter | table tab |
| ⌘⇧F | Filter tables | Database |
| ⌘T / ⌘W | New query tab / close tab | Database |
| ⌃Tab / ⌃⇧Tab | Next / previous tab | Database |
| ⌘↵ / ⇧⌘↵ | Run statement / run all | editor |
| ⌘E | Open source row | document |
| ⌘⇧S | Take snapshot | document |
| ⌘S | Apply pending changes | everywhere |
| ⌘D | Add to favorites | Database |

Overrides live in `keybindings.json` (`{ "commandId": "Mod+Shift+X" | null }`); menu-bound shortcuts apply after a restart.

## CLI

```sh
databee mysql://user:pass@127.0.0.1:3306/wordpress
databee -H 127.0.0.1 -P 52958 -u db -d db --name "Shop (DDEV)" --password-env DB_PASS
echo "$PASS" | databee -u root -d local --socket ~/Library/.../mysqld.sock --password-stdin --save-password
databee --help
```

Password sources, in order: `--password` / `--password-stdin` / `--password-env`, then the
URL, then the Keychain (account `user@host:port`), then a prompt in the app.

### Install

Two symlinks point at the debug build, so every `bun run build:app` is picked up without reinstalling:

| Link | Created by | Use |
| --- | --- | --- |
| `~/.local/bin/databee` → `databee.app/Contents/MacOS/databee` | `scripts/install-cli.sh` | the `databee` CLI command, from any shell (`~/.local/bin` must be on `PATH`) |
| `~/Applications/databee.app` → `databee.app` | `scripts/install-app.sh` | the GUI launcher in Spotlight, Launchpad and the Dock |

Both scripts take an optional path to another `databee.app` (e.g. a release build).

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
(`settings.json`, `keybindings.json`, `ui_state.json`, `favorites.json`, `connections.json`, `history.json`,
`attributions.json` for manual plugin assignments, and the optional `entities.toml` / `plugins.toml`). Logs are in `~/Library/Logs/com.philippstracker.databee/`.

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

### Plugin registry

The plugins lens knows WordPress core, WooCommerce, Subscriptions, PayPal Payments, Jetpack, Action Scheduler,
WooPay, Payoneer, WooPayments and Stripe out of the box (`src-tauri/src/wp/plugins.toml`). Add `plugins.toml`
to the config directory to teach it more; user entries win over built-in ones.
```toml
[[plugin]]
slug = "acme-widgets"
name = "Acme Widgets"
option_prefixes = ["acme"]
meta_prefixes = ["_acme"]
hook_prefixes = ["acme"]
log_sources = ["acme-widgets"]
tables = ["acme_"]
gateways = ["acme-pay"]
```
