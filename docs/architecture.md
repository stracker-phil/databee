# Architecture

DataBee is a single-window desktop app built with Tauri 2. A Rust process owns every piece of logic:
connections, SQL, rules and presets, persistence, secrets, and the CLI. A React 19 webview renders
state and forwards user intent. This page covers the overall shape. The layer pages go into detail:
[backend](backend.md), [database](database.md), [WordPress](wordpress.md), [frontend](frontend.md),
[Stage hook](stage.md).

## Design rules

These rules hold everywhere in the code. Keep them when you change things.

1. **Rust owns the logic; React only renders.** SQL is built in Rust, and so are planned edits, WordPress
   parsing and safety checks. The frontend never assembles a write statement on its own. It asks Rust to
   *plan* a change, shows the planned SQL, and sends it back to be applied.
2. **IPC only in `api.ts` files, one per module.** Only a module's API file imports `@tauri-apps/api` or a
   `@tauri-apps/plugin-*` package and calls `invoke` or `listen`. The core's file is `src/api/index.ts`;
   a module's is `src/<module>/api.ts`. The rest of the frontend imports `api` from there, which is also
   what UI tests mock. Stage's is `src/stage/api.ts` (`stageApi`).
   IPC types are camelCase, with one deliberate exception: rule types (`src-tauri/src/rules/`) keep the
   snake_case keys of the preset TOML format, so projects, exports and the page store a rule verbatim.
3. **Errors are data.** Every command returns `Result<T, AppError>`, which serializes as `{ kind, message, code, account }`.
   The UI branches on `kind` (e.g. `password_required`, `confirmation_required`).
4. **Passwords never touch files, logs or the webview.** They live in the `Secret` type. It redacts in
   `Debug`, is zeroed on drop and has no `Serialize` impl. Passwords are stored in the OS keychain and handed to a connect call by
   request id.
5. **Show what is stored.** Values travel losslessly: numbers and dates as strings, binary values as a hex
   preview, PHP-serialized data byte for byte. Labels may be shown next to a raw value, never instead of it.
   Presets, projects and the WordPress layer follow [`.claude/rules/presets.md`](../.claude/rules/presets.md).
6. **No silent writes.** Grid edits, WordPress edits and maintenance actions all go through the same
   *plan → review → apply in one transaction → undo* pipeline. Destructive SQL typed in the editor asks
   for confirmation first.

## Process model

```
┌──────────── DataBee.app ─────────────────────────────────────────────┐
│                                                                      │
│  Webview (React)                       Rust core (tokio runtime)     │
│  ┌──────────────────────┐   invoke    ┌───────────────────────────┐  │
│  │ components/          │ ──────────▶ │ #[tauri::command]s        │  │
│  │ state/store.ts       │             │  commands.rs  db_commands │  │
│  │ commands/ (registry) │ ◀────────── │  wp/commands  stage/cmds  │  │
│  │ api/index.ts, api.ts │   events    ├───────────────────────────┤  │
│  └──────────────────────┘ launch,     │ AppState: stores, sessions│  │
│                           command,    │ db/ (dyn Database)       │  │
│                           stage:*     │ wp/ (lens)  store/ (JSON) │  │
│                                       │ secrets (Keychain)        │  │
│                                       └──────┬───────┬────────┬───┘  │
└──────────────────────────────────────────────┼───────┼────────┼──────┘
                         Unix socket (ipc.rs)  │       │ sqlx   │ HTTP 127.0.0.1
   `databee …` (2nd process) ──────────────────┘       ▼        ▼ (stage/)
                                                MySQL/MariaDB   Stage presenter app
```

- **One instance.** The first `databee` process binds a Unix socket in the config directory. Later launches,
  such as `databee mysql://…` from a script, send their parsed request over that socket and exit
  ([backend: CLI and IPC](backend.md#cli-and-single-instance-handoff)).
- **Launch requests are pulled, not pushed.** Rust queues launch requests and emits a bare `launch` event. The
  page calls `take_launch_events`. The page can't miss the initial request even though it subscribes late.
- **The native menu emits `command` events** carrying a command id. The page runs that id through the same
  registry that handles shortcuts and the palette.
- **The Stage hook** is an optional local HTTP API through which a presenter app drives the UI
  ([stage.md](stage.md)).

## Repository map

```
src-tauri/                 Rust crate `databee` (lib `databee_lib`)
  src/lib.rs               startup, plugin setup, command registration, run loop
  src/address/             addresses: parse, format, resolve against a session
  src/cli.rs ipc.rs        argv parsing, single-instance socket
  src/launch.rs            queue of launch requests + one-shot passwords
  src/state.rs session.rs  AppState, open sessions
  src/schema.rs            per-session schema cache: tables, columns, keys, foreign keys, structures
  src/value/               Value, ValueKind, ColumnKind, RowKey: what a cell is and where it came from
  src/lens/                the lens seam: SchemaLens (keys, foreign keys, same-name inference), result kinds
  src/rules/               presets and projects: rule types, TOML parsing, the preset catalog, projects.json,
                           effective rules, the rules lens and reference coverage
  src/commands.rs          settings, keybindings, favorites, history, misc
  src/db_commands.rs       connect, schema, execute, browse
  src/db/                  MySQL driver, SQL lexer, classify, safety, cell decoding
  src/decode/              PHP serialize and JSON value trees, leaf edits
  src/wp/                  WordPress screens over the presets (until U12): documents, views, commands, snapshots
  src/store/               JSON stores in the config dir
  src/stage/               Stage plugin: HTTP API, request hub, commands
  src/test_support.rs      fixture-server helpers for tests (`test-util`)
  src/secrets/             SecretStore trait; Keychain and test MemoryStore
  src/menu.rs              native menu (MENU_ITEMS)
  src/window.rs            window theme and bring-to-front
  src/error.rs connection.rs  AppError, ConnectionSpec, Secret
  src/lock.rs              poison-tolerant std lock access
  presets/                 built-in presets: wordpress, woocommerce, action-scheduler (docs/wordpress.md)
  tests/                   core integration tests (Docker, #[ignore]); the WordPress screens' are in src/wp/
  tauri.conf.json          window, bundle and identifier config
src/                       React app
  main.tsx modules.ts       boot; the install list (core, then one line per module)
  api/                     the core's IPC file (index.ts) + types generated from Rust (ts-rs)
  state/                   zustand store: store.ts (composition root), slices/, selectors, session events
  commands/                command registry, scopes, default shortcuts
  registry/                extension points modules register into (settingsPages.ts)
  components/              UI; components/wp/ is WordPress mode, components/settings/ the Settings dialog
  stage/                   page side of the Stage hook, its Settings page, installStage()
  lib/                     pure helpers (formatting, SQL ranges, theme, addresses, values)
  styles/                  schemes/ (one file per color scheme), theme.css (derived tokens), fonts.css, app.css
  assets/fonts/            bundled Geist and JetBrains Mono (OFL)
  test/                    api mock, fixtures
dev/                       Docker fixtures for integration tests
scripts/                   install links; dev/ drives the app headlessly
.claude/                   rules and the ship-step skill for Claude Code
```

## Modules

A module is an optional capability (Stage, the WordPress lens) that the core works without. It keeps its
code in its own folder and contributes to the core only through registries, so removing it is deleting the
folder and its install line. The core never imports a module; modules import the core.

- **Frontend.** `src/modules.ts` is the install list: `installModules()` runs the core's install, then one
  call per module (`installStage()` from `src/stage/install.ts`, `installWp()` from
  `src/components/wp/install.ts`), then `installCommands()`. `main.tsx` calls it once before the first
  render. The registries a module can add to:
  - store slices: `registerSlice(slice)` in `src/state/store.ts`, typed by augmenting `Store`;
  - Settings pages: `registerSettingsPage(page)` in `src/registry/settingsPages.ts`;
  - command scopes: `registerScope(def)` in `src/commands/scopes.ts`.
- **Backend.** A module is an inline Tauri plugin with one install line in `lib.rs`
  ([ADR 0010](adr/0010-inline-tauri-plugins.md), [backend](backend.md#plugins)): `stage` and `wp`.

Not yet separated: the `wp` store slice and the WordPress commands live in the core (`src/state/slices/wp.ts`,
`src/commands/index.ts`), and `App.tsx` renders `WpWorkspace`.
See [ADR 0015](adr/0015-frontend-registries.md).

## Addresses

Every view has one address ([ADR 0021](adr/0021-addresses.md)), a URL-like name that the status bar shows and copies
and that back/forward, and later deep links, the CLI and MCP, pass around:

```
databee://<connection>/<database>/                       home
databee://<connection>/<database>/<table>                rows of a table
databee://<connection>/<database>/<table>?col=val&col2   rows with a structured filter (bare `col` = IS NULL)
databee://<connection>/<database>/<table>/<key>          object (`?view=document|rows`, `?compare=<key2>`)
databee://<connection>/<database>/q/<saved-query-id>     saved query
databee://<connection>/<database>/?find=<value>          find everywhere
databee://<connection>/<database>/?project               project page
```

- `<connection>` is the saved connection's **slug** (`RecentConnection.slug`): unique, editable in Settings →
  Connections, by default the `--name` given on launch, else `<host>-<port>-<database>`.
- `<table>` is the real table name with its prefix; `<key>` the single-column primary key value. Composite
  keys use the filter form. A table literally named `q` is written `%71`, so it never reads as a saved query.
- Every part is percent-encoded: everything but `A-Z a-z 0-9 - . _ ~`, as UTF-8 bytes in uppercase hex.
- Rust owns the model (`src-tauri/src/address/`: `Address::parse`, `Display`, `resolve`); the page keeps a
  port (`src/lib/address.ts`) because it builds an address on every navigation. A shared fixture
  (`src/test/fixtures/addresses.json`, written by a Rust test) keeps both equal.
- Free SQL filters are not addressable: a tab filtered by typed SQL keeps the table's address with a
  `· filtered` hint.

## Values and the lens

Every value the UI shows is a `Value` (`src-tauri/src/value/`): the raw `Cell`, a `ValueKind` (text,
number, timestamp, primary key, reference, no reference, serialized, binary, null, empty; money, enum and
secret come from rules), its `Provenance` (table, column, row key) and optional secondary text. There is no
status kind: color comes only from value styles.

- **Per column, not per cell.** A result carries one `ColumnKind` per column (`ResultSet.kinds`), the
  cells that decode as PHP-serialized or JSON (`ResultSet.serialized`), the few cells whose row gives
  them another kind (`ResultSet.cell_kinds`: conditional references, meta-key kinds), the cells a value
  style colors (`ResultSet.styles`) and, for a browse page, every row's key (`BrowsePage.keys`). The page builds a cell's `Value` from them (`valueAt` in
  `src/lib/values.ts`), so 500 rows don't carry 500 copies of each kind. This is the one rule the page
  ports from Rust, for payload size: `value::value_at` (`ResultValues` for many cells) is the reference
  implementation that Rust callers outside the page use, and tests on both sides keep them equal.
- **The lens seam.** `lens::Lens` answers `kinds(table, columns)`, `references_from(table)`,
  `references_to(table)` and `cells(columns, result)` (per-row kinds and styles). `SchemaLens` answers
  from the schema cache: primary keys, declared foreign keys and same-name inference
  ([database.md](database.md#references-and-kinds)). `rules::lens::RulesLens` wraps it with the effective
  rules of presets and the project; `lens::build` picks it when the rules change anything.

## Rules and presets

`src-tauri/src/rules/` is the generic configuration layer ([presets.md](presets.md)). A preset is a TOML file
of rules for tables named without their prefix: key/value tables, references, entities, kinds, value styles,
patterns, pinned queries, anomalies, actions and find sources. No application name appears in its Rust code;
WordPress, WooCommerce and Action Scheduler are preset files ([wordpress.md](wordpress.md)).

- `types.rs`, `kinds.rs`, `queries.rs`, `actions.rs`: the rule types. Each has one serialized form, the TOML
  shape, shared by preset files, projects and the page (exported to `types.gen.ts`). Shapes with exclusive
  fields (`KindRule`, `Action`) read a flat `*Fields` struct and convert with `try_from`.
- `preset.rs`: `parse(file, src)` reads every rule on its own, so a `PresetError` names the file, the rule
  (`[[reference]] #3 (line 40)`) and the field; `Check` covers what serde cannot (placeholders, names that
  must exist in the preset). `to_toml` writes a preset back.
- `placeholder.rs`: `{p}`, `{np}`, `{id}`, `{owner.<column>}`, `{meta.<key>}`, `{param.<name>}`, and which
  of them a piece of SQL may use.
- `catalog.rs`: `PresetCatalog` in `AppState`: the built-ins (`BUILT_IN`, `include_str!` of
  `src-tauri/presets/*.toml`) plus `<config dir>/presets/*.toml`, reread when the directory's files or
  mtimes change; `requires_order` resolves `requires`.
- `prefix.rs`: `suggest_prefix`, the shortest prefix under which a preset's marker tables exist.
- `project.rs`, `project_change.rs`: `Project` per connection slug and database in `ProjectsStore`
  (`projects.json`, in `AppState`), edited by `ProjectChange`s that are checked against the schema cache;
  `Project::export` writes it as a preset file.
- `applied.rs`: `Applied`, which presets apply and the prefix and network prefix their table names
  resolve under (also used to check project changes).
- `condition.rs`: `when`/`to_when` and value-style matches compiled once (`Matcher`; the `regex` crate for
  LIKE and REGEXP) and evaluated in memory over a page.
- `effective.rs`: `EffectiveRules` (applied presets ⊕ project, real table names, precedence order, the
  `fk → owner` reference of each owned `[[meta]]` table, kind lookups shared with the lens) and its
  cache in `Session.ext`, keyed by the schema (`Arc`), the preset list (`Arc`) and the project (value), so
  DDL, a refresh, a preset file change or a project change rebuilds it. `RuleSources` (from
  `AppState::rules`) is what `Session::browse`, `Session::execute` and `table_references` take.
- `lens.rs`: `RulesLens` (see the lens seam above).
- `coverage.rs`: `reference_coverage`, read-only counts of how many rows of one reference resolve
  (`CoverageQuery` builds the SQL from the reference and its rule's conditions).

## A request, end to end

Running `SELECT * FROM wp_options` in a query tab:

1. `QueryTab` calls `store.runQuery(tabId, sql)`.
2. The store calls `api.execute(sessionId, sql, false)`, which wraps `invoke("execute", …)`.
3. `db_commands::execute` looks up the `Session`. If `confirm_destructive` is on, it checks the SQL with
   `db::safety::destructive_reason` and returns `ConfirmationRequired` for statements like `DROP` or an
   unscoped `DELETE`.
4. `Database::execute` (`MySqlSession`) runs the whole script on the session's dedicated editor connection with the
   configured timeout and row limit. It decodes each cell into a typed `Cell` and returns one `ResultSet`
   per statement.
5. Rust records a history entry and returns `ExecuteResponse` to the store. If an error has kind
   `confirmation_required`, the store opens a confirm dialog and re-runs with `confirmed = true`.
6. `ResultsGrid` renders the last result set, virtualized.

Editing a cell is a separate round trip. `queueChange` calls `plan_change`, which reads the current value and
returns `{description, statements, undo}`. The plan appears in the pending tray, and ⌘S calls
`apply_changes`, which runs every pending statement in one transaction. Undo replays the `undo` statements.

## Platform

The code base targets **macOS on Apple Silicon** only. The platform-specific parts are listed in
[adapting.md](adapting.md#platform-specific-code): the Keychain, the config path, Unix sockets, window vibrancy
and the dev scripts. Most of the code is portable: the database layer, the WordPress layer, the stores and
nearly all of the UI.
