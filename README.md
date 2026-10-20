# DataBee

My personal SQL client for MySQL and MariaDB. I build WordPress and WooCommerce plugins and spend my days
inspecting **local** databases (DDEV, Docker); DataBee shows what is stored there, raw, and makes the rows
navigable as objects. It is not a tool for site admins or shop owners.

It is published as source, as-is: no releases, no binaries, no support. It runs on macOS on Apple Silicon.
If you want it on your machine or your platform, point a coding agent at [AGENTS.md](AGENTS.md).

![The readout of a WooCommerce database](docs/images/readout.png)

## Features

**SQL client**
- **CLI-first:** scripts and other apps open DataBee with connection details. A second launch hands its
  request to the running window.
- **Schema tree** with a filter. Paged table view with sorting, resizable columns and a filter field: smart
  (`home_` finds rows where any column contains it, numbers are IDs: `3229`, `3229, 3227`, `3220-3229`,
  `'5'` is text) or a SQL `WHERE` condition, switched with ⌃⌥X. Structure view with columns, indexes and DDL, and per
  column the rules that apply to it (kinds, references, value styles), plus the table's entities and actions.
- **SQL editor** (CodeMirror):
  - MySQL autocomplete;
  - run the statement at the cursor or the whole script;
  - cancel, timeout and row limit;
  - asks before `DROP`, `TRUNCATE` and unscoped `DELETE`/`UPDATE`.
- **Results grid:** virtualized and typed, with inline editing. Copy cells or rows as TSV or JSON.
- **Value column:** Enter on serialized data or long text opens it as a column with a tree of PHP-serialized
  or JSON values, including serialized data nested in strings, whose leaves you can edit. Values with broken
  string lengths are decoded leniently, with a warning. An open value column reads its value again when the data
  changes (⌘R, an applied edit).
- **Saved queries** (⌘S in a query tab, `q:` in the palette; pinned with Count or Rows), per-server **query
  history**, and a **⌘K command palette** for commands, tables, records, saved queries and recent items.
- **Value counts** on every grid column (right-click its header): each stored value with its count; a click
  filters the table, or adds a `WHERE` to a plain single-table query.
- **Passwords** live in the macOS Keychain and never in files or logs.
- **Connection watch:** when DDEV or Docker stops, a warning next to the connection says since when and why;
  what is loaded stays readable, and Retry (or coming back to the window) reconnects without a password prompt,
  with the current database and session variables such as `sql_mode` restored. It also retries by itself
  (after 10 s, 30 s, then every 60 s); live mode and the table tab check pause while the connection is lost.

**Rows as objects** (WordPress, WooCommerce and Action Scheduler come as presets)
- **Readout:** the home page of every database, opened on connect. A status line (host, database, prefix,
  presets, server version, and values the presets pin, such as the site URL or the autoloaded bytes), the
  anomalies the presets report (failed or overdue scheduled actions, expired transients, HPOS drift, log
  errors) with their fix, which you review first, the newest rows of each entity (orders, posts, users), your
  pinned queries with their counts or rows, the values you watch, and the rows you opened last. Counts refresh
  after every apply.
- **Watched values:** right-click a grid cell, a document value or a node of a decoded value and choose Watch
  on readout; the readout shows it as stored now, with where it is read from.
- **Documents:** any row with a primary key opens as a document: its columns, its meta rows, every row that
  references it, rows matched by preset patterns (logs, scheduled actions) and its timestamps as events.
  References open as a stack of columns; ⌥ peeks.
- **Pane stack:** the columns of an object tab show as resizable panes side by side (1 to 3, Settings →
  Appearance, "Panes side by side"); a trail above them lists every column, hidden ones too. ⌘⇧↵ maximizes
  the active pane.
- **Click indicators:** a passive icon next to a link shows what a click does: a chevron opens a column beside
  it, an arrow moves the whole view or opens a tab.
- **Linked table highlight:** hover a table in the sidebar and its rows and the references into it light up
  in the open panes; everything else dims.
- **Table icons:** `[[icon]]` rules give a table one of a curated set of icons, shown before its name, its
  references and its row keys. Set one per project on the Project page or in the Structure view.
- **Live mode** (`w`): the row is read again every 2 s; a changed value flashes and keeps its old value struck
  through, new and removed rows are marked, and a change list under the document says what changed when.
  Snapshots (`s`) of a row or a saved query's rows show the same marks later ("Changes since snapshot…"), and
  `?compare=<key>` shows two rows side by side with their differences.
- **Table tabs check their rows:** every 4 s the active tab reads the shown rows again by primary key (one
  statement); changed and removed rows are marked, a bar counts them with Refresh, and ⌥ on a marked value
  shows old → new. Nothing changes on screen by itself. Turn it off in Settings → Queries, "Live check in
  table tabs".
- **Pinned queries** from the presets: orders, subscriptions, users, customers, guests, products, sessions,
  scheduled actions, content and the options queries open as query tabs with their SQL (`q:` in the palette).
- **Raw first:** real column names, meta keys and status slugs, with amounts and times next to them where a
  preset says what they are. Secrets stay masked until clicked.
- **Find:** `#4711`, `@jane` and `=<value>` in the palette find rows by id, people and any stored value.
- **No silent writes:** every edit lands in a pending tray with its SQL. Edits include fields, meta rows,
  serialized values edited as a tree (re-serialized with correct byte lengths) and the presets' actions
  (order status, autoload, retry or cancel a scheduled action). The tray applies everything in one
  transaction (⌘S), and you can undo it.
- **Presets:** describe your plugin's tables in a preset of your own; the Project page adds references,
  kinds, value styles and table icons per database, and an accent of its own so you see which database is
  open. The project also keeps your saved queries and watched values.

**Presentations:** an optional local API lets the [Stage](docs/stage.md) presenter app drive the UI in a
demo.

## Screenshots

An order as a document: its columns, meta rows and every row that references it, with the customer open in
a second pane.

![An order and its customer as two panes](docs/images/document.png)

The same order in Rows view: each related table as a grid.

![An order in Rows view](docs/images/rows.png)

The Project page: the rules a preset and the project apply to the database.

![The Project page](docs/images/project.png)

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
- `find(connection, database, query)`: rows as the palette finds them: `#4711` (rows with that id and
  the rows that reference them), `@jane` (people), `=value` (find everywhere) or free text of 6+ characters
  (search columns and external ids);
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
| ⌘⇧P | Switch connection or database | everywhere |
| ⌘[ / ⌘] | Back / forward in the tab's history | everywhere |
| ⌘= / ⌘- / ⌘0 | Zoom in / out / reset | everywhere |
| ⌘R | Refresh the current view | everywhere |
| ⌘S | Apply pending changes; in a query tab with nothing pending, save the query | everywhere / editor |
| ⌘N / ⌘, / ⌘⌃S | New connection / Settings / toggle sidebar | everywhere |
| ? | Keyboard shortcuts | everywhere |
| ⎋ | Restore a maximized pane, else close the rightmost column | everywhere |
| ⌘F | WHERE filter | Table |
| ⌃⌥X | Switch the WHERE filter between smart and SQL | Table |
| ⌘⇧F | Filter tables | Database |
| ⌘T / ⌘W | New query tab / close tab | Database |
| ⌃Tab / ⌃⇧Tab | Next / previous tab | Database |
| ⌘↵ / ⇧⌘↵ | Run statement / run all | editor |
| P | Pin the saved query (focus in the results) | editor |
| J / K, ↓ / ↑ | Next / previous value | object |
| H / L, ← / → | Column to the left / right | object |
| ↵ | Open the focused reference in a new column | object |
| ⌘⇧↵ | Maximize or restore the active pane | object |
| ⌘⌥← / ⌘⌥→ | Focus the pane on the left / right | object |
| ⌘⇧W | Close the active pane and the panes opened from it | object |
| ⌘⌥↵ | Open the active pane's row in a new tab | object |
| ⌘1 / ⌘2 | Document / Rows view | object |
| W / S / X | Live mode / snapshot / dismiss change marks | object |
| ⌥ (hold) | Peek the hovered or focused reference | object, grid |
| Y / ⇧Y | Copy the value / copy it as a SQL condition | object, grid |
| E / F / R | Edit the value / filter rows by it / add a reference from it | object, grid |

Every shortcut can be rebound or unbound in Settings → Shortcuts. Overrides are stored in `keybindings.json`.

## Files

Config and data live in `~/Library/Application Support/com.philippstracker.databee/`: settings, keybindings,
recent connections, history, UI state, projects (rules, saved queries, watched values), recent objects
(`recent_objects.json`), snapshots (`snapshots/`), Stage's settings (`stage.json`) and user presets. Logs are in
`~/Library/Logs/com.philippstracker.databee/`. Details are in [docs/backend.md](docs/backend.md#persistence).

### Presets

What tables mean comes from the rules in presets: references, kinds, value styles, icons, entities, queries,
anomalies and actions. WordPress, WooCommerce and Action Scheduler are built in; a TOML file in `presets/` in
the config directory adds a preset or replaces a built-in of the same name. See [docs/presets.md](docs/presets.md) for the format and
[Built-in presets](docs/presets.md#built-in-presets) for the built-ins.

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

## Build or adapt it

There are no install instructions for people on purpose. [AGENTS.md](AGENTS.md) tells a coding agent how
to clone, build and verify DataBee, and [docs/adapting.md](docs/adapting.md) lists every place that is
specific to macOS, the name or the WordPress presets, for a port to Linux or Windows or a rename. The
[technical documentation](docs/README.md) covers the architecture, each layer, testing and tooling.

For example, in [Claude Code](https://claude.com/claude-code):

```text
Clone github.com/stracker-phil/databee, build it and link the databee command.
Port DataBee to Linux. Use libsecret instead of the Keychain and keep all tests green.
Add SQLite support, including opening a .sqlite file from the CLI.
```

## License

[MIT](LICENSE)

## Credits

- [Geist](https://github.com/vercel/geist-font) by Vercel and [JetBrains Mono](https://github.com/JetBrains/JetBrainsMono)
  by JetBrains are bundled under the SIL Open Font License 1.1 (license files in `src/assets/fonts/`).
- The Ayu Mirage and Ayu Light color schemes follow the [Ayu](https://github.com/ayu-theme/ayu-colors) palette.
