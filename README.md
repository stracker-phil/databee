# DataBee

A minimalistic SQL client for MySQL and MariaDB, built for WordPress and WooCommerce developers who inspect
their **local** databases (DDEV, Docker, Local). It shows what is stored, not a friendlier version of it. It
is not a tool for site admins or shop owners.

Built with Tauri 2, Rust and React 19. Runs on macOS (Apple Silicon). The code is documented so you can
point a coding agent at it and adapt it to your platform or needs. See [Adapt it](#adapt-it).

![WordPress mode: dashboard of a local WooCommerce store](docs/images/wordpress-dashboard.png)

![Database mode: wp_options in the table view](docs/images/database-table.png)

## Features

**SQL client**
- **CLI-first:** scripts and other apps open DataBee with connection details. A second launch hands its
  request to the running window.
- **Schema tree** with a filter. Paged table view with sorting, resizable columns and a `WHERE` filter that
  also takes IDs (`3229`, `3229, 3227`, `3220-3229`). Structure view with columns, indexes and DDL.
- **SQL editor** (CodeMirror):
  - MySQL autocomplete;
  - run the statement at the cursor or the whole script;
  - cancel, timeout and row limit;
  - asks before `DROP`, `TRUNCATE` and unscoped `DELETE`/`UPDATE`.
- **Results grid:** virtualized and typed, with inline editing. Copy cells or rows as TSV or JSON.
- **Value inspector:** decodes PHP-serialized and JSON values into a tree, including serialized data nested
  in strings. Values with broken string lengths are decoded leniently, with a warning.
- **Favorites**, per-server **query history**, and a **⌘K command palette** for commands, tables, records and
  recent items.
- **Passwords** live in the macOS Keychain and never in files or logs.

**WordPress mode** (on by default for WordPress databases)
- **Dashboard:** orders, autoload weight, failed scheduled actions and expired transients, with one-click
  fixes that you review first.
- **Lists** that follow wp-admin: content, users (including guest customers), options, orders,
  subscriptions, products, sessions and scheduled actions. Each has facets, search and a side pane.
- **Purpose-built pages.**
  - **Orders:** addresses, line items, totals, a PayPal fee waterfall, attribution, and a timeline that merges
    notes, actions, logs and refunds.
  - **Customers, products, actions and sessions:** each has a page of its own.
- **Raw first:** real column names, meta keys and status slugs, with amounts and times next to them where a
  preset says what they are. Secrets stay masked until clicked.
- **Exploration:** hover peeks on linked records, "find this value everywhere", snapshots with diffs,
  side-by-side comparison, and a live mode that highlights changing fields.
- **No silent writes:** every edit lands in a pending tray with its SQL. Edits include fields, serialized
  values edited as a tree (re-serialized with correct byte lengths), order status and autoload. The tray
  applies everything in one transaction (⌘S), and you can undo it.
- **Presets:** WordPress, WooCommerce and Action Scheduler are TOML presets; describe your plugin's tables in
  a preset of your own.

**Presentations:** an optional local API lets the [Stage](docs/stage.md) presenter app drive the UI in a
demo.

## Install

Requirements: macOS on Apple Silicon (developed and tested on macOS 27), [rustup](https://rustup.rs) (stable), [bun](https://bun.sh)
and Xcode Command Line Tools. Docker is needed only for the integration tests.

```sh
git clone https://github.com/stracker-phil/databee.git && cd databee
bun install
bun run app                 # debug build of DataBee.app, then open it
scripts/install-cli.sh      # optional: link the `databee` command into ~/.local/bin
scripts/install-app.sh      # optional: link DataBee.app into ~/Applications (Spotlight, Dock)
```

For a release bundle, run `bun run tauri build`. It writes `src-tauri/target/release/bundle/macos/DataBee.app`.

## CLI

```sh
databee mysql://user:pass@127.0.0.1:3306/wordpress
databee -H 127.0.0.1 -P 52958 -u db -d db --name "Shop (DDEV)" --password-env DB_PASS
echo "$PASS" | databee -u root -d local --socket /path/to/mysqld.sock --password-stdin --save-password
databee --help
```

With DataBee running, `open` and `command` act on its window:

```sh
databee open 'databee://shop/wordpress/wp_posts/42'       # any address (copy it from the status bar)
databee open order 4711 --connection shop                 # <entity> <id>; default: the active connection
databee open order 4711 --background                      # without bringing DataBee to the front
databee command view.sidebar                              # a command id, as in keybindings.json
```

`open` starts DataBee when it is not running; `command` then fails with "DataBee is not running".

DataBee takes the password from the first source that has one:
1. `--password`, `--password-stdin` or `--password-env`;
2. the URL;
3. the Keychain (account `user@host:port`);
4. a prompt in the app.

### Deep links

Addresses are also links: `open 'databee://shop/wordpress/wp_posts/42'` in the terminal, or clicking such a
link in another app, opens it in DataBee. When no session of that connection is open but a saved
connection has the slug, DataBee connects first (Keychain password, else it asks). Links need the bundled
app (`bun run build:app`, then start it once or run `scripts/install-app.sh`); `bun tauri dev` gets none.

## Claude Code

`databee mcp` is a read-only MCP server on stdio. Add it once:

```sh
claude mcp add databee -- databee mcp
```

Its tools:
- `connections`: the saved connections and their slugs;
- `document(address)`: a row as its document, every value with its provenance;
- `rows(address, limit)`: the rows of a table or row address;
- `query(connection, sql, limit)`: SELECT, SHOW, DESCRIBE, EXPLAIN and WITH … SELECT only; anything else
  is refused before it reaches the server;
- `references(connection, database)`: every reference between the tables, with its source;
- `link(address)`: the canonical `databee://` link;
- `current`, `open(address, focus)` and `command(id)`: act on the running app.

The data tools connect on their own, from the saved connections and the Keychain. They work without
the app, so `connections` lists only connections that were opened in DataBee once. Nothing writes to a
database, and project rules cannot be edited from MCP. An agent's answer can be opened in the app with
`databee open <address>` or as a deep link (see [CLI](#cli)).

## Shortcuts

| Shortcut | Command | Where |
|---|---|---|
| ⌘K / ⌘P | Command palette / open table or record | everywhere |
| ⌘1 / ⌘2 | WordPress / Database mode | everywhere |
| ⌘[ / ⌘] | Back / forward | WordPress |
| ⌘= / ⌘- / ⌘0 | Zoom in / out / reset | everywhere |
| ⌘R | Refresh the current view | everywhere |
| ⌘S | Apply pending changes | everywhere |
| ⌘F / ⌘⇧F | WHERE filter / filter tables | Database |
| ⌘T / ⌘W | New query tab / close tab | Database |
| ⌃Tab / ⌃⇧Tab | Next / previous tab | Database |
| ⌘↵ / ⇧⌘↵ | Run statement / run all | editor |
| ⌘D | Add to favorites | Database |
| ⌘E / ⌘⇧S | Open source row / take snapshot | WordPress record |

Every shortcut can be rebound or unbound in Settings → Shortcuts. Overrides are stored in `keybindings.json`.

## Files

Config and data live in `~/Library/Application Support/com.philippstracker.databee/`: settings, keybindings,
recent connections, favorites, history, snapshots, UI state, projects and user presets. Logs are in
`~/Library/Logs/com.philippstracker.databee/`. Details are in [docs/backend.md](docs/backend.md#persistence).

### Presets

What tables mean (references, kinds, entities, queries) comes from presets. WordPress, WooCommerce and Action
Scheduler are built in; a TOML file in `presets/` in the config directory adds a preset or replaces a built-in
of the same name. See [docs/presets.md](docs/presets.md) for the format and
[docs/wordpress.md](docs/wordpress.md) for the built-ins. The old `entities.toml` is no longer read.

```toml
[preset]
name = "ppcp-vault"
requires = ["wordpress"]
marker_tables = ["ppcp_vault_tokens"]   # without prefix

[[reference]]
from = "ppcp_vault_tokens.user_id"
to = "users.ID"

[[entity]]
name = "vault_token"
table = "ppcp_vault_tokens"
summary = ["customer_id"]
```

## Development

| Command | What it does |
|---|---|
| `bun run test` | Vitest UI tests and Rust unit tests |
| `bun run test:db` | starts the MariaDB 11.4 and MySQL 8.4 fixtures (`dev/`) and runs the integration tests |
| `bun run lint` | `tsc` and `clippy -D warnings` |
| `bun run build:app` + `scripts/dev/launch.sh` | debug build, started next to the terminal without taking focus |

The [technical documentation](docs/README.md) covers the architecture, each layer, testing and tooling.

## Adapt it

The repo is set up for coding agents:
- `CLAUDE.md` and [`.claude/rules/`](.claude/rules/) hold the conventions.
- A `ship-step` skill tests, builds and commits each step. Before every commit, a `clean-up-code` skill
  keeps the code self-explanatory.
- [docs/adapting.md](docs/adapting.md) lists every place that is specific to macOS, the name or the
  WordPress layer.

Open the repo in [Claude Code](https://claude.com/claude-code) and ask for what you need:

```text
Port DataBee to Linux. Use libsecret instead of the Keychain and keep all tests green.
Add SQLite support, including opening a .sqlite file from the CLI.
Rename the app to "ShopLens" and remove the Stage integration.
```

## License

[MIT](LICENSE)

## Credits

- [Geist](https://github.com/vercel/geist-font) by Vercel and [JetBrains Mono](https://github.com/JetBrains/JetBrainsMono)
  by JetBrains are bundled under the SIL Open Font License 1.1 (license files in `src/assets/fonts/`).
- The Ayu Mirage and Ayu Light color schemes follow the [Ayu](https://github.com/ayu-theme/ayu-colors) palette.
