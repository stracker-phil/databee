# Architecture

DataBee is a single-window desktop app built with Tauri 2. A Rust process owns every piece of logic:
connections, SQL, rules and presets, persistence, secrets, and the CLI. A React 19 webview renders
state and forwards user intent. This page covers the overall shape. The layer pages go into detail:
[backend](backend.md), [database](database.md), [presets](presets.md), [frontend](frontend.md),
[Stage hook](stage.md).

## Design rules

These rules hold everywhere in the code. Keep them when you change things.

1. **Rust owns the logic; React only renders.** SQL is built in Rust, and so are planned edits, decoding of
   serialized values and safety checks. The frontend never assembles a write statement on its own. It asks Rust to
   *plan* a change, shows the planned SQL, and sends it back to be applied.
2. **IPC only in `api.ts` files, one per module.** Only a module's API file imports `@tauri-apps/api` or a
   `@tauri-apps/plugin-*` package and calls `invoke` or `listen`. The core's file is `src/api/index.ts`;
   a module's is `src/<module>/api.ts`. The rest of the frontend imports `api` from there, which is also
   what UI tests mock. Stage's is `src/stage/api.ts` (`stageApi`).
   IPC types are camelCase, with one deliberate exception: rule types (`src-tauri/src/rules/`) keep the
   snake_case keys of the preset TOML format, so projects, exports and the page store a rule verbatim.
3. **Errors are data.** Every command returns `Result<T, AppError>`, which serializes as
   `{ kind, message, code, account, reason?, sessionReset?, field? }` (`ErrorJson` in `error.rs`).
   The UI branches on `kind` (e.g. `password_required`, `confirmation_required`). Rule form errors
   (`AppError::InvalidField`) add `field: { path, message }`.
4. **Passwords never touch files, logs or the webview.** They live in the `Secret` type. It redacts in
   `Debug`, is zeroed on drop and has no `Serialize` impl. Passwords are stored in the OS keychain and handed to a connect call by
   request id.
5. **Show what is stored.** Values travel losslessly: numbers and dates as strings, binary values as a hex
   preview, PHP-serialized data byte for byte. Labels may be shown next to a raw value, never instead of it.
   Presets, projects and every view of their results follow [`.claude/rules/presets.md`](../.claude/rules/presets.md).
6. **No silent writes.** Grid edits, document edits and rule actions all go through the same
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
│  │ commands/ (registry) │ ◀────────── │  readout/     stage/cmds  │  │
│  │ api/index.ts, api.ts │   events    ├───────────────────────────┤  │
│  └──────────────────────┘ launch,     │ AppState: stores, sessions│  │
│                           command,    │ db/ (dyn Database)        │  │
│                           remote,     │ rules/      store/ (JSON) │  │
│                           session_    │ secrets (Keychain)        │  │
│                           health,     │                           │  │
│                           stage:*     │                           │  │
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
  src/remote.rs            requests the page answers (open, command, current), by id with a timeout
  src/pending.rs           PendingReplies: requests waiting for the reply with their id (remote::Bridge, Stage hub)
  src/deep_link.rs         databee:// links from other apps
  src/mcp/                 `databee mcp`: the read-only MCP server (rmcp, stdio), headless
  src/state.rs session.rs  AppState, open sessions
  src/health/              connection health: pings, the guard that refuses calls while a server is gone,
                           retries without a password prompt, `session_health` events (database.md)
  src/schema.rs            per-session schema cache: tables, columns, keys, foreign keys, structures
  src/value/               Value, ValueKind, ColumnKind, RowKey: what a cell is and where it came from
  src/lens/                the lens seam: the Lens trait, SchemaLens (keys, foreign keys, same-name inference),
                           kinds of a one-table result; knows nothing of rules
  src/rules/               presets and projects: rule types, TOML parsing, the preset catalog, projects.json,
                           effective rules, the rules lens (and which lens a database gets), result kinds by
                           column origin, reference coverage
  src/query/               Ctx and Rows: bounded reads of one database through its schema, rules and lens,
                           shared by documents, search, the readout and value counts
  src/document/            documents: one row with its meta rows, incoming references, patterns and events
  src/search/              the palette's search: ids, people, external ids, find everywhere
  src/saved/               saved queries: project, global and preset pinned ones; the favorites migration
  src/value_counts.rs      value counts of one column
  src/table_check.rs       the live check of table tabs: checksums of the shown rows by key, changed rows read again
  src/statement_counts.rs  test-only: statements per object view, palette, readout and coverage (database.md)
  src/dev.rs               debug builds only: window frame, background launch, eval for scripts/dev
  src/commands.rs          settings, keybindings, history, misc
  src/db_commands.rs       connect, schema, execute, browse
  src/db/                  MySQL driver, SQL lexer, classify, safety, cell decoding
  src/decode/              PHP serialize and JSON value trees, leaf edits
  src/readout/             the readout: the home page of a database (status, anomalies, recently created, watched, pinned, recent)
  src/store/               JSON stores in the config dir
  src/stage/               Stage plugin: HTTP API, request hub, commands
  src/test_support.rs      fixture-server helpers for tests (`test-util`)
  src/secrets/             SecretStore trait; Keychain and test MemoryStore
  src/menu.rs              native menu (MENU_ITEMS)
  src/window.rs            window theme and bring-to-front
  src/error.rs connection.rs  AppError, ConnectionSpec, Secret
  src/lock.rs              poison-tolerant std lock access
  presets/                 built-in presets: wordpress, woocommerce, action-scheduler (docs/presets.md)
  tests/                   integration tests of the database layer (Docker, #[ignore]): mysql.rs, changes.rs,
                           sql_mode.rs, health.rs; the others sit next to their module
  tauri.conf.json          window, bundle and identifier config
src/                       React app
  main.tsx modules.ts       boot; the install list (core, then one line per module)
  api/                     the core's IPC file (index.ts) + types generated from Rust (ts-rs)
  state/                   zustand store: store.ts (composition root), slices/, selectors
  commands/                command registry, scopes, default shortcuts
  registry/                extension points modules register into (settingsPages.ts)
  components/              UI; components/readout/ the home page, components/settings/ the Settings dialog
  stage/                   page side of the Stage hook, its Settings page, installStage()
  lib/                     pure helpers (formatting, SQL ranges, theme, addresses, values)
  styles/                  schemes/ (one file per color scheme), theme.css (derived tokens), fonts.css, app.css
  assets/fonts/            bundled Geist and JetBrains Mono (OFL)
  test/                    api mock (apiMock.ts), fixtures, the IPC boundary test
dev/                       Docker fixtures for integration tests
scripts/                   install links; dev/ drives the app headlessly
.claude/                   rules and the ship-step skill for Claude Code
```

## Modules

A module is an optional capability (Stage) that the core works without. It keeps its
code in its own folder and contributes to the core only through registries, so removing it is deleting the
folder and its install line. The core never imports a module; modules import the core.

- **Frontend.** `src/modules.ts` is the install list: `installModules()` runs the core's install, then one
  call per module (`installStage()` from `src/stage/install.ts`), then `installCommands()`. `main.tsx` calls it once before the first
  render. The registries a module can add to:
  - store slices: `registerSlice(slice)` in `src/state/store.ts`, typed by augmenting `Store`;
  - Settings pages: `registerSettingsPage(page)` in `src/registry/settingsPages.ts`;
  - command scopes: `registerScope(def)` in `src/commands/scopes.ts`.
- **Backend.** A module is an inline Tauri plugin with one install line in `lib.rs`
  ([ADR 0010](adr/0010-inline-tauri-plugins.md), [backend](backend.md#plugins)): `stage`.

See [ADR 0015](adr/0015-frontend-registries.md). WordPress is no module: it is three preset files.

## Addresses

Every view has one address ([ADR 0021](adr/0021-addresses.md)), a URL-like name that the status bar shows and copies
and that back/forward, deep links (`databee://` URLs from other apps), the CLI (`databee open`) and MCP pass around:

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
  ports from Rust, for payload size: `value::ResultValues` is the reference implementation that Rust
  callers outside the page use, and tests on both sides keep them equal.
- **The lens seam.** `lens::Lens` answers `kinds(table, columns)`, `references_from(table)`,
  `references_to(table)` and `cells(columns, result)` (per-row kinds and styles). `SchemaLens` answers
  from the schema cache: primary keys, declared foreign keys and same-name inference
  ([database.md](database.md#references-and-kinds)). `rules::lens::RulesLens` wraps it with the effective
  rules of presets and the project; `rules::lens::build` picks it when the rules change anything, and
  `rules::lens::for_database` and `table_references` load the lens of a database.
  `lens::annotate_table` gives a result whose columns all come from one table its kinds with a lens the
  caller holds. `rules::annotate::annotate_browse` does that for a browse page and adds its row keys;
  `rules::annotate::annotate_execute` gives query results their kinds by column origin, loading a lens per
  database; `rules::annotate::annotate_query` does the same for one result with a lens the caller already
  holds (the readout's saved queries and snapshots of saved queries use the `query::Ctx`'s).
- **Dependencies.** `lens/` depends on the schema and values only; `rules/` builds on `lens/`, and
  `query/` on both. `document/`, `search/`, `readout/` and `value_counts.rs` read through `query::Ctx`;
  `document/` exports only what it owns.

## Rules and presets

`src-tauri/src/rules/` is the generic configuration layer ([presets.md](presets.md)). A preset is a TOML file
of rules for tables named without their prefix: key/value tables, references, entities, kinds, value styles, table icons,
patterns, pinned queries, anomalies, actions and find sources. No application name appears in its Rust code;
WordPress, WooCommerce and Action Scheduler are preset files ([presets.md](presets.md#built-in-presets)).

- `types.rs`, `kinds.rs`, `icons.rs`, `queries.rs`, `actions.rs`: the rule types (`icons.rs` also resolves the icon of each table, `table_icons`). Each has one serialized form, the TOML
  shape, shared by preset files, projects and the page (exported to `types.gen.ts`). Shapes with exclusive
  fields (`KindRule`, `Action`) read a flat `*Fields` struct and convert with `try_from`.
- `preset.rs`: `parse(file, src)` reads every rule on its own, so a `PresetError` names the file, the rule
  (`[[reference]] #3 (line 40)`) and the field; `Check` covers what serde cannot (placeholders, names that
  must exist in the preset). `to_toml` writes a preset back.
- `placeholder.rs`: `{p}`, `{np}`, `{id}`, `{owner.<column>}`, `{meta.<key>}`, `{param.<name>}`, which
  of them a piece of SQL may use, and `render` for the value placeholders. Prefixes are filled in only by
  `Applied::expand` (`applied.rs`) and `query::Ctx::preset_sql` (qualified with the database first).
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
  LIKE and REGEXP) and evaluated in memory over a page; `conditions_text` says them as SQL-like text for
  the Project page, `conditions_sql` as exact SQL terms (coverage, incoming groups, action plans, search).
- `effective.rs`: `EffectiveRules` (applied presets ⊕ project, real table names, precedence order, the
  `fk → owner` reference of each owned `[[meta]]` table, kind lookups shared with the lens, `rule_for`, the
  rule behind a `ResolvedReference`; `owned_meta`, `entities_of` and `patterns_of` for the meta tables,
  entities and patterns of one table) and its
  cache in `Session.ext`, keyed by the schema (`Arc`), the preset list (`Arc`) and the project (value), so
  DDL, a refresh, a preset file change or a project change rebuilds it. `effective::load` returns a
  database's schema and its effective rules; `rules_info` builds `RulesInfo` from them. `RuleSources` (from
  `AppState::rules`) is what `Session::browse`, `Session::execute` and `table_references` take.
- `lens.rs`: `RulesLens`, `build`, `for_database`, `table_references` (see the lens seam above).
- `annotate.rs`: kinds, row keys, per-cell kinds and styles of browse pages and query results.
- `coverage.rs`: `reference_coverage`, read-only counts of how many rows of one reference resolve
  (`CoverageQuery` builds the SQL from the reference and its rule's conditions), and `coverage_preview`,
  the same count for an unsaved rule on a checked copy of the project (`project_change::put_reference`
  stores it and returns its id).
- `overview/`: `project_rules` (`for_database` loads its inputs), everything the Project page lists (references of every origin, switched-off
  ones included, kinds, styles, presets, the tables rules can name).
- `profile.rs`: `TableProfile` and `for_table`, a table's rules per column (kinds, references, value styles)
  and its meta-key rules, incoming references, entities and actions; backs `table_rules` for the Structure view.
- `draft.rs`: `rule_draft`, a value's column as rules name it and, on a meta value, its meta key.

## Documents

`src-tauri/src/document/` builds the document of any row with a primary key from the effective rules and the
schema alone; no code names an application. `document(session_id, address, database)` takes a row address
(`databee://…/<table>/<key>`, or a table address whose filter covers a composite key) or an entity shorthand
`<entity> <key>` (`order 4711`) in `database` (default: the session's current one).

```rust
Document { address, entity, header: Summary, groups: Vec<Group>, alternatives: Vec<Address>,
           actions: Vec<ObjectAction> }
Summary { values: Vec<SummaryValue { name, value: Value }> }
Group { id, table, via: GroupLink, shape: Fields | Meta | Rows, columns, total, rows: Vec<GroupRow>, capped,
        rows_address, error }
GroupLink = Own | Meta { fk, key, value } | Incoming { reference, label } | Pattern { rule, sql } | Events
GroupRow { key: Option<RowKey>, fields: Vec<(String, Value)> }
```

**Groups, in order** (`build.rs`):
1. **own** (`own.rs`): the row, read in one query together with the `filter` of every entity of its table; the
   row's `entity` is the first whose filter holds (filtered entities come first), and `header` is the row's
   [summary](#summaries).
2. **meta** (`meta.rs`): one group per owned `[[meta]]` table, rows in stored order (the meta table's primary
   key), at most 50. When capped, the keys the document reads by name (header, `{meta.…}` of patterns,
   currencies) are read in one more query.
3. **incoming references** (`incoming.rs`): every reference that points at the table, in the lens's order
   (`Lens::references_to`: rule references in precedence order, then declared foreign keys, then same-name
   inference), except the `fk` of the table's own meta tables. A reference whose `to_when` does not hold on
   the row is left out. Source rows are those the lens gives this reference: the rule's `when` holds, no
   conditional rule that comes first on the same column holds, and a rule after an unconditional one on the
   same column is left out. A column that references its own table never finds the row itself. At most 20
   rows; `columns` picks the group's columns, and names that are keys of the source's `[[meta]]` table are
   pivoted in with one query per group (`pivot.rs`; a missing key is NULL).
4. **patterns** (`patterns.rs`): `[[pattern]]` rules of the table, merged by name and table (OR), each
   applying when its `owner_when` holds and none of its `{owner.…}`/`{meta.…}` values is NULL or `''`; ordered
   by `order`, at most 20 rows (fewer with a smaller `limit`).
5. **events** (`events.rs`): every timestamp-kind value of the own row, the meta rows and the incoming groups,
   and of each pattern row its `order` column, sorted. A value without a stored zone (`local`, or none)
   sorts by its wall-clock text; zero dates are left out. Each row is one value with its provenance.

Groups with no rows stay in the list with `total: 0`. A group whose query fails (a timeout, broken rule SQL)
carries `error`; the rest of the document stands. `rows_address` is the group's rows as a structured filter
when `=` conditions select exactly them.

**Values.** Every field is a `Value` with provenance (`lens::annotate_table` gives a result the kinds of
browse pages). A money value whose currency is not in its own row takes the first listed name that is a
column of the owner row (for meta rows the document's row; for pivoted values the group row), else a meta
key of the document. Outgoing references get `exists` from one `SELECT DISTINCT` per target table for all
values that point into it.

**Queries** are bounded by the rules, never by the rows: one for the row, one per meta table, one per
source table of the incoming groups (several references from one table share a `UNION ALL`,
`query::Ctx::capped_each`), one per pattern group (a `COUNT(*)` more for each capped group), one per pivoted group
and one per referenced table. The measured counts are in [database.md](database.md#statement-counts). Each uses the session's query
timeout on the metadata pool.

**Peek** (`peek.rs`): `peek(session_id, address)` returns `Peek { address, key_column, summary, matches }`
without building a document. For a row address: the row in one query (with its entity, as for `own`) and its
summary (meta rows only when it needs them); `address` is the row's canonical address. A table address that
is not one row (a reference to a non-key column, e.g. `guest_orders`: `wp_wc_orders?customer_id=0&billing_email=…`)
peeks at the set: `SELECT … ORDER BY <key> LIMIT 6` with the entity tests; exactly one match is peeked as that
row (with its row `address`); otherwise `summary` is empty and `matches: PeekMatches { total, rows:
Vec<PeekMatch { address, key, summary }> }` holds the first 5 (a `COUNT(*)` only when there are more); no
match is `total: 0`, not an error. The row a reference starts from is not part of the address, so a
reference to its own table may list that row among the matches. `key_column` is the single-column key row
addresses name, for the peek's `via … → table.column` line.

**Referencing** (`build.rs`): `referencing` gives the incoming reference and pattern groups of a row without
its document, for the palette's search (meta rows only when a pattern of the table needs them); `own::resolve`
reads the row of an address with its entity, for documents, peeks and the search.

**Entity shorthand** (`shorthand.rs`): the tables that define the entity are tried with the key (a table
only for the entity, without `filter`, before filtered ones); the first that holds it wins and the others are
`alternatives`.

**Edits.** A value's provenance names its row: an edit is a core `setCell` on it (meta values included, the
meta table's primary key is the key), deleting a meta row is `deleteRow`, and adding a meta key is `addRow`
of `(fk, key, value)` into the meta table (`GroupLink::Meta`), all through `plan_change` like grid edits
([database.md](database.md#editing-rows)).

**Live mode, snapshots and compare** (`diff.rs`, `watch.rs`, `snapshot.rs`, `compare.rs`): a document reduced
to its stored cells (`FlatDocument`), diffed by provenance into a `DocDelta` of changed values and added and
removed rows; watches keep the last one per row in `Session.ext`, snapshots store it on disk
(`store/snapshots.rs`), compare aligns two rows of one table ([backend.md](backend.md#watches-and-snapshots)).

**Actions** (`actions/`): `actions` lists the table's row-scope `[[action]]` rules whose `when` holds on the
row, as `ObjectAction { name, preset, kind: update | delete | sql, undo, params, writes }` (`writes`: per step
the stored table and its set columns, or that it deletes rows or runs free SQL); each `ActionParam` carries
the `choices` its `choices_sql` returns (one query per listed param). `table_actions(session_id, database,
table)` lists the `scope = "table"` ones. `preview_action(session_id, address, action, params)` plans one on a
row address, or on a table address without a filter for a table action, without queueing it: the
`ActionPreview` lists every row and value it writes (old → new) and the session keeps the plan;
`queue_action_preview(session_id, id)` queues that plan and returns a `PlannedChange` like `plan_change`, so
the pending tray applies and undoes it. `plan_action` plans and queues in one step (`actions/plan.rs`,
`actions/preview.rs`, see [presets.md](presets.md#action)).

### Summaries

`summary.rs`: one rule gives the summary of a row wherever it shows: the document header,
peeks, palette hits and the readout's timeline and recent objects. With an entity `summary` (the row's entity,
picked as `own` picks it): every name in order, a column of the row (any case, named as the table spells it),
else a meta key of the row's `[[meta]]` tables (the first stored row of the key, the meta tables in rule
order), else NULL, so a configured field that is empty shows instead of dropping out. Without one: the first
4 columns outside the primary key that are neither NULL nor `''`, in schema order. Money takes its currency
from the row's own columns, then its meta keys (also for a meta value in the summary). `summary::rule` is the
pure rule; `of_document` feeds it the document's own row and meta rows; `of_rows` summarizes rows of one table
together and reads the meta keys they need (summary names that are no column, and money currencies when a
summary shows money without one) with one `fk IN (…) AND key IN (…)` query per meta table, only for rows that
need them; `entities` tests the entity filters of found rows by key (one query, only when the table has
filtered entities); `of_keys` reads rows again by key with their entities (for document group rows, which may
hold only some columns). The document header additionally gets `exists` on its reference values.

## Readout

`src-tauri/src/readout/` builds the home page of a database (`databee://<slug>/<db>/`):
`readout(session_id, database) -> Readout { address, status, anomalies, recentlyCreated, watched, pinned, recent }`.
Every part has its own timeout (`Part { items, error }`, `RecentlyCreated { items, entities, error }`), so a
slow anomaly or a broken pinned query shows as an error line and the rest stands. The status line comes from
the session and the effective rules plus the saved queries pinned as values; anomalies are the applied
presets' `[[anomaly]]` rules with a non-zero value; `recentlyCreated` is one timeline of the newest rows over
every entity with a `created` column (`CreatedItem { entity, table, address, created, summary }`, with each
entity's query as `CreatedEntity { entity, preset, table, sql, countInWindow, error }`); watched are the
project's watched values as they are now (`readout/watched.rs`, one query per table they name); pinned are the saved
queries pinned as count or rows, each line with `tables`, the tables its SQL names after `FROM` or `JOIN`
(`db::lexer::read_tables`); recent are the row addresses `remember_object` kept per project in
`recent_objects.json` (`store::recent_objects`, at most 20), each with its peek's summary. Details: [presets.md](presets.md#the-readout). The page shows it in the readout tab
(`src/components/readout/Readout.tsx`, [frontend.md](frontend.md#readout)), opened on connect.

## Palette search

`src-tauri/src/search/` answers the palette's row searches (`palette_search(session_id, database, query)`,
the MCP `find` tool) from the effective rules and the schema alone. `search::parse` reads the input:

| Input | `Query` | Sections |
|---|---|---|
| `#<digits>` | `Id` (`ids.rs`) | `objects with id <n>`: each entity table with a single-column key that holds the key (one `own` query per table, its summary as a peek reads it); `rows that reference <n>`: the incoming reference and pattern groups of the first 5 found rows (`document::referencing`; meta rows only when a pattern of the table needs them) |
| `@<text>` (2+ characters) | `People` (`text.rs`) | `people matching <text>`: entities with `people = true`, on their `search` columns (`LIKE` contains) and `search_exact` columns (`=`); names that are no column are keys of the entity's own `[[meta]]` table (`pk IN (SELECT fk … )`) |
| free text (6+ characters, no prefix) | `Text` (`text.rs`) | `external id <text>`: the targets of rule references that are not the target table's key (`=` plus the `to_when` conditions), a meta row as its owner row; then `objects matching <text>`: every entity's `search`/`search_exact` |
| `=<value>` (2+ characters) | `Find` (`find.rs`) | one section per `table.column`: the value column of every `[[meta]]` table (exact), the `[[find]]` sources (their `match`), the entities' `search` (contains) and `search_exact` columns; the sources of one table share one `UNION ALL` (`Ctx::rows_per_filter`) |

Without entities, `#` reads every base table with a single-column key and fewer than 1M estimated rows
(`PLAIN_TABLE_ROWS`). Each query runs with `QUERY_TIMEOUT` (3 s, or the query timeout when lower); a failed
or timed-out query drops only its own hits. Each section has at most `SECTION_CAP` (20) hits, and each row
appears once (the first section keeps it). A `PaletteHit { section, address, source, title, detail }` opens
`address` (a meta value's owner row); `title` is the opened row's key and `detail` the values of its
[summary](#summaries), led by the matched value when the summary does not show it (for a meta value its key
and value), all `Value`s with provenance. Hits of found rows get their row's entity from the search query
itself (`document::Selection` adds the entity tests) or from `summary::entities`; `rows that reference <n>`
reads its rows again by key per table (`summary::of_keys`), since group rows may hold only some columns.

## A request, end to end

Running `SELECT * FROM wp_options` in a query tab:

1. `QueryTab` calls `store.runQuery(tabId, sql)`.
2. The store calls `api.execute(sessionId, sql, false)`, which wraps `invoke("execute", …)`.
3. `db_commands::execute` calls `run_query`. If `confirm_destructive` is on and the run is not confirmed, it
   checks the SQL with `db::safety::destructive_reason` and returns `ConfirmationRequired` for statements
   like `DROP` or an unscoped `DELETE`. Only then does it look up the `Session` (`sessions.get`), so the gate
   holds even for a stale session id.
4. `Session::execute` runs the script through `health::Guarded` (fails at once while the connection is
   lost) on `MySqlSession`, the session's dedicated editor connection, with the configured timeout and row
   limit. Each cell decodes into a typed `Cell`, one `ResultSet` per statement; `annotate_execute` adds the
   kinds and `CellCache::keep` keeps long cells for `fetch_cell`.
5. Rust records a history entry and returns `ExecuteResponse` to the store. If an error has kind
   `confirmation_required`, the store opens a confirm dialog and re-runs with `confirmed = true`.
6. `ResultsGrid` renders the last result set, virtualized.

Editing a cell is a separate round trip. `queueChange` calls `plan_change`, which reads the current value and
returns `{description, statements, undo}`. The plan appears in the pending tray, and ⌘S calls
`apply_changes`, which runs every pending statement in one transaction. Undo replays the `undo` statements.

## Platform

The code base targets **macOS on Apple Silicon** only. The platform-specific parts are listed in
[adapting.md](adapting.md#platform-specific-code): the Keychain, the config path, Unix sockets, window vibrancy
and the dev scripts. Most of the code is portable: the database layer, the rules layer, the stores and
nearly all of the UI.
