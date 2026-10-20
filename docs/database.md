# Database layer

`src-tauri/src/db/` is the MySQL/MariaDB driver and the SQL tooling around it. The commands that use it live
in `src-tauri/src/db_commands.rs`.

## Modules

| File | Responsibility |
|---|---|
| `db/mod.rs` | The `Database` trait every caller uses (`&dyn Database`, [ADR 0011](adr/0011-database-trait-object.md)), the `open` factory (MySQL only today) and re-exports |
| `db/types.rs` | Shared DTOs (`ColumnMeta` with its `origin`, `ResultSet` with its `id`, `kinds`, `serialized` and the IPC-skipped `full_texts`, `ExecuteResponse` with its `failure: StatementFailure`, `TableInfo`, `TableStructure`, `BrowseRequest`, `BrowsePage` with `table` and `keys`, `AppliedFilter`, `FilterKind`) |
| `db/dialect.rs` | `Dialect { backslash_escapes }` (`Dialect::MYSQL`, or `from_sql_mode`): `quote_ident` (backticks), `sql_string`, `like` / `like_contains`, `table_ref`, `system_schemas`; `escape_like` |
| `db/browse.rs` | `browse_sql` (the table view's paged SELECT plus the filter it applied), `page_limit`, `MAX_PAGE_ROWS` |
| `db/mysql/mod.rs` | `MySqlSession`, the only driver, implementing `Database`: the metadata pool, the editor connection, `connect_options`. It owns all I/O. |
| `db/mysql/editor.rs` | `EditorConnection`: the editor connection's lock, thread id, current database, `reconnect`, `kill_query` and `kill_connection` |
| `db/mysql/kill.rs` | `Killer`: `KILL QUERY <id>` or `KILL <id>` on a one-off connection, for any server thread id; `spawn_kill_query` fires one in the background |
| `db/mysql/introspect.rs` | `databases`, `tables`, `structure`, `completion` over the pool; rows are read by column alias |
| `db/mysql/meta_rows.rs` | `rows` (a bound `information_schema` query within the timeout), `text`, `text_or_empty` (read by column alias), shared by `introspect.rs` and `keys.rs` |
| `db/mysql/keys.rs` | Indexes and foreign keys: `table_keys` for one table (inside `structure`), `database_keys` for every table of a database (`Database::keys`) |
| `db/mysql/script.rs` | `run_script` (text protocol, one `ResultSet` per statement, column recovery; returns a `ScriptOutcome` of the finished results and the failing statement, `into_results` for callers that need all of it), `scalar`, `thread_id`, `read_dialect` |
| `db/mysql/timeouts.rs` | `connect_within`, `query_within`, and `pooled_within`, which reads the pool connection's thread id, kills its query on timeout and closes the connection |
| `db/fake.rs` | `FakeDatabase`, an in-memory `Database` for unit tests (canned results matched by SQL fragment, recorded `apply` batches); behind `cfg(any(test, feature = "test-util"))` |
| `db/lexer.rs` | A small MySQL-aware scanner, not a parser. `scan` is an iterator of borrowed `Piece { kind, text, start, depth }`. It knows the three quote styles, backslash escapes, the `#`, `-- ` and `/* */` comments, executable `/*! */` comments and `@` variables. `split_statements`, `words`, `pieces` (comments included; `Piece::span`, `is_comment`, `is_open`, `is_symbol`), `is_terminated`, `is_blank` |
| `db/narrow.rs` | `refusal` and `narrowed`: one more condition on a plain single-table `SELECT` ([Narrowing a query](#narrowing-a-query)) |
| `db/classify.rs` | What a statement is: `verb_index` (the deciding word, also after a `WITH` clause), `returns_rows`, `changes_schema` (a script with `CREATE`, `ALTER`, `DROP`, `RENAME` or `TRUNCATE`), and `is_read_only` (every statement is `SELECT`, `SHOW`, `DESCRIBE`/`DESC`, `EXPLAIN` or `WITH … SELECT`; refuses `SELECT … INTO`, `FOR UPDATE`/`FOR SHARE`/`LOCK IN SHARE MODE`, `EXPLAIN ANALYZE` of a non-read, `CALL`, `SET`, `HANDLER` and every other verb, naming the statement and why; functions called by a `SELECT` are not inspected) |
| `db/safety.rs` | `destructive_reason` (which statements need confirmation) |
| `db/cell.rs` | `Cell`: decodes text-protocol bytes by column `Family`, computed once per result set from the type name; `CellLimit` cuts long text to a preview |
| `db/cell_cache.rs` | `CellCache`: numbers result sets per session and keeps the full text of their `LongText` cells for `fetch_cell` |
| `db/id_filter.rs` | Primary-key shorthand in the WHERE field: `3229`, `3229, 3227`, `3220-3229` |

Outside `db/`:
- `connection.rs` has `ConnectionSpec` and `Secret`.
- `session.rs` has `Session { id, spec, db: Box<dyn Database>, ext, plans, cells }` and the `Sessions` map
  (`ddl_writes()` counts user scripts with DDL). `ext` is a
  type-keyed `Extensions` map where a module keeps per-session data that dies with the session
  ([ADR 0013](adr/0013-session-extensions.md)); the rules keep their effective-rules cache there, the core its
  `SchemaCache` ([Schema cache](#schema-cache)).
- `error.rs` maps `sqlx::Error` to `AppError`.

## Connection model

Every connection opens **two channels**:

- **The editor connection.** This is one `MySqlConnection` behind a `tokio::sync::Mutex`. Every user query and
  `USE` runs on it, so session state survives between runs: `USE`, `@vars`, `SET`, open transactions and
  temporary tables all persist. Its server thread id comes from `CONNECTION_ID()` and is kept for cancelling.
- **The metadata pool.** This is a `MySqlPool` of 3 connections. It serves schema introspection, table
  browsing, documents, the readout and applying planned changes. Pool connections don't share the editor's
  `USE`, so these queries always use qualified `` `db`.`table` `` names. The one exception is the readout's
  saved queries, written without a database: they run as `USE <db>; <query>` (`readout/sql.rs`), so a pool
  connection may keep another default database afterwards, which no other pool query relies on.

`connect` opens the pool and the editor connection concurrently (`tokio::try_join!`) under one
`connect_timeout`. The editor connection has no statement cache (`statement_cache_capacity(0)`), so the
statements that column recovery prepares are closed right away instead of staying open on the server.

Other details:
- **Connect options** are built from `ConnectionSpec`: socket or host and port, user and optional password
  (`connect_options`). The database is added per connection (`with_database`): the spec's database for the
  pool and the first editor connection, the current database on reconnect, none for `KILL`. Server defaults are left alone on purpose: sqlx is told not to change `sql_mode`, `time_zone` or
  `PIPES_AS_CONCAT`. Instead the editor connection reads `@@SESSION.sql_mode` on connect and reconnect, and
  `Database::dialect` reports whether backslashes escape (`NO_BACKSLASH_ESCAPES`); browse and every
  planned literal and identifier use it ([ADR 0018](adr/0018-escaping-follows-sql-mode.md)). A user's own
  `SET sql_mode` on the editor connection is picked up only on the next reconnect.
- **Timeouts:** every server call is bounded, so a half-dead connection can't hang a command.
  - `connect_timeout_secs` covers connecting, reconnecting the editor and the one-off `KILL` connection
    (`connect_within`; a timeout is a `connection` error).
  - `query_timeout_secs` covers `execute`, `browse`, `fetch` and `apply` (read per call) and the metadata
    calls `use_database`, `databases`, `tables`, `structure` and `completion` (`query_within`, using the value
    stored on `MySqlSession` at connect time; `browse`, `fetch` and `apply` use `pooled_within`). A timeout is a
    `timeout` error.
  - When an `execute` times out, DataBee kills the whole editor connection, reconnects it, and returns
    `timeout` with `sessionReset: true` when the reconnect succeeded: temporary tables, `@vars` and open
    transactions are gone, and the UI says so in a toast. A timed-out `browse`, `fetch` or `apply` spawns
    `KILL QUERY <thread id>` for its pool connection (`pooled_within` reads `CONNECTION_ID()` after acquiring
    it; closing the socket alone leaves the query running on the server) and closes the connection instead
    of returning it. The kill runs in the background and is not awaited; a failure is logged.
    If `apply` times out after `COMMIT` was sent, it returns a `database` error saying the outcome is
    unknown.
- **Cancel** runs `KILL QUERY <editor thread id>` on a one-off connection (`Killer`), so it works while the running query
  holds the editor lock and every pool connection is busy. `cancel_query` returns the `KILL` error, if any. A
  cancelled query on MariaDB returns error 1317, which maps to `cancelled`. MySQL instead reports an
  interrupted `SLEEP` as a result row.
- **Reconnects:** when `execute` fails with a `connection` error (including a server-side kill or
  `wait_timeout`, see the error mapping in [backend.md](backend.md#errors)), the editor connection is rebuilt
  with the current database. The query itself is not retried. A failed reconnect is logged; the next run
  tries again.
- **TLS:** none. sqlx is compiled without a TLS backend. `mysql-rsa` handles `caching_sha2_password` over
  plaintext connections by fetching the server's RSA key unencrypted, so a man in the middle could swap the key
  and read the password. That is fine on loopback and sockets, the intended use; sqlx has no option to refuse
  it for remote hosts. To add TLS:
  1. Enable `tls-rustls` in `Cargo.toml`.
  2. Add `ssl_mode`/CA fields to `ConnectionSpec`.
  3. Set them in `connect_options`.

## Running a query

1. **Confirmation.** `db_commands::execute` checks `destructive_reason`. It runs only when the
   `confirm_destructive` setting is on and the call isn't already confirmed. A flagged statement returns
   `ConfirmationRequired(reason)`.
2. **Safety rules** (`safety.rs`) look at each statement's bare words, compared case-insensitively. Text
   inside quotes, comments and `@variables` doesn't count, so `SET a = @where` has no `WHERE`. The body of an
   executable comment (`/*!50000 … */`, `/*M! … */`) is code, as it is for the server. The rule is picked by
   the statement's verb (`classify::verb_index`): its first word, or for `WITH …` the first statement keyword
   at the top level after the CTEs. Flagged:
   - `DROP`
   - `TRUNCATE`
   - `DELETE` or `UPDATE` without a `WHERE` at the verb's parenthesis depth; a `WHERE` inside a subquery
     doesn't scope the statement, and `LIMIT` alone doesn't either
   - `ALTER … DROP`
   - `RENAME TABLE`
   - `CREATE OR REPLACE TABLE` and `CREATE OR REPLACE VIEW` (MariaDB)

   `REPLACE` is not flagged.
3. **Execution.** `run_script` sends the **whole script as one multi-statement text-protocol call**
   (`sqlx::raw_sql(..).fetch_many`). Each statement result closes one `ResultSet`, with columns, rows,
   `affectedRows` and `lastInsertId`.
4. **Row limit.** Rows beyond `row_limit` are counted but not decoded: `totalRows` stays exact and
   `truncated` is set. The query editor has no paging. Text longer than `maxCellChars` arrives as a
   preview (see [Cell decoding](#cell-decoding)).
5. **Partial results.** The server stops a script at its first failing statement. `run_script` keeps the
   result sets of the statements before it and reports the failure as `StatementFailure { index, error }`
   (`index` is 0-based; rows the failing statement had already streamed are dropped). The convention:
   - the first statement failed (nothing finished): `execute` returns the error as `Err`, as for a single
     statement;
   - a later statement failed: `execute` returns `Ok` with the earlier `results` and `failure` set. The UI
     shows those results under an `ErrorPanel` titled "Statement N failed" (N = `index + 1`).

   A connection error in either case still rebuilds the editor connection. `browse` and `fetch` need the
   whole result and turn any failure into an `Err` (`ScriptOutcome::into_results`).
6. **Empty results.** `run_script` collects the stream into one `ResultSet` per statement (`ScriptResults`).
   The text protocol sends no column metadata for an empty result. When the lexer's split matches the result
   count, a result without columns and with 0 affected rows whose statement `classify::returns_rows` accepts
   is `PREPARE`d to recover its column names (`needs_column_recovery`). A failed `PREPARE` is logged at debug
   level. It accepts `SELECT` (also in parentheses), `SHOW`, `DESC`/`DESCRIBE`, `EXPLAIN`, `VALUES`,
   `TABLE`, `CALL`, `CHECK`, `ANALYZE`, `OPTIMIZE`, `REPAIR`, `CHECKSUM`, `HANDLER`, and `WITH …` when the
   statement after the CTEs is `SELECT`, `TABLE` or `VALUES`.

   The split has known limits: backslash is always an escape (`NO_BACKSLASH_ESCAPES` is ignored), `"` always
   quotes a string (`ANSI_QUOTES` is ignored), and there is no `DELIMITER` support. Where they apply, the
   count can differ and the column recovery is skipped. After a failing statement the counts never match,
   so a partial result keeps the columns the server sent.
7. **History.** A history entry is written for every run, including failed ones. It stores the whole script;
   its `error` is the run's error or, for a partial result, the failing statement's error.
8. **Current database.** `SELECT DATABASE()` refreshes the session's current database after every run,
   also after a failed statement (a `USE` before it still counts), inside the same timeout. The UI follows a `USE` this way. If this probe fails, the results are still
   returned and the current database stays as it was.

### Cell decoding

`cell.rs` keys on the first word of the column type name. A cell is serialized as `{ "t": kind, "v": value }`.

| Column types | `Cell` | Why |
|---|---|---|
| any NULL | `null` | |
| BOOLEAN, TINYINT…BIGINT, FLOAT, DOUBLE, DECIMAL, YEAR | `number` (string) | 64-bit ids and decimals keep full precision |
| DATE, TIME, DATETIME, TIMESTAMP | `temporal` (string) | zero dates like `0000-00-00` survive |
| JSON | `json` | |
| BIT | `bytes` | |
| BINARY, VARBINARY, BLOBs, GEOMETRY | `text` if the value is valid UTF-8 without control characters, else `bytes { len, hexPreview }` | `*_bin` collations set the binary flag on ordinary text |
| everything else | `text`, or `bytes` for invalid UTF-8 | |

Full binary values never cross IPC. Only the first 16 bytes are sent, as hex.

**Long text.** `decode(family, raw, limit)` takes a `CellLimit` (the `maxCellChars` setting,
`Settings::cell_limit`, passed into `Database::execute` and `Database::browse`). A `text` value longer than
the limit becomes `longText { len, preview }`: `len` counts characters, `preview` is the first `limit`
characters (cut at a character boundary). `number`, `temporal` and `json` cells are never cut. `fetch` uses
`CellLimit::NONE`, because its callers (documents, the readout, change planning) need every value in full.

The full text does not cross IPC with the result. `run_script` records it in `ResultSet.full_texts` (skipped by
serde and ts-rs), and `Session::execute` / `Session::browse` (behind the `execute` and `browse_table` commands)
pass each result set to the session's `CellCache::keep`, which gives it an `id` (a per-session counter) and moves the full texts into the cache.
`fetch_cell(sessionId, resultId, row, col)` returns one of them. The cache holds at most 64 MiB of text; above
that it drops whole result sets, oldest first (never the one being filled). A dropped or unknown cell returns
`invalid_input` "Result expired; run the query again". The cache dies with the session.

## Browsing tables

`browse_table` → `Session::browse` → `Database::browse` → `browse_sql`:

```sql
SELECT * FROM `db`.`t` [WHERE (<columns>) AND (<filter>\n)] [ORDER BY `col` ASC|DESC] LIMIT <limit + 1> OFFSET <offset>
```

- **Paging:** fetching one extra row tells whether there is a next page. The UI asks for `rowLimit` rows;
  `page_limit` clamps the requested limit to `1..=MAX_PAGE_ROWS` (500) and `BrowsePage.limit` reports the
  clamped size, which the UI pages by.
- **Sorting:** one column at a time, quoted with `Dialect::quote_ident`.
- **Structured `where`:** `BrowseRequest.where` is a list of `ColumnEquals` (`{ column, value }`) sent by
  `openTableTab`, `showRows` and `filterRows` ([frontend.md](frontend.md#opening-rows)). `browse::columns_filter` renders each as
  `` `col` = 'value' `` (`Dialect::quote_ident`, `Dialect::sql_string`), or `` `col` IS NULL `` for a `null`
  value, joined with `AND` into one group placed before the typed filter. Values are always quoted strings,
  never bare literals: MySQL compares them as strings for text columns and coerces them for numeric ones.
  `BrowsePage.columnsFilter` (`kind: columns`) reports the group as it ran: `sql` quoted, `label`
  unquoted for the filter bar.
- **Filter:**
  - If the table has a single-column primary key and the text matches the id grammar, the text becomes
    `pk = n`, `pk IN (…)` or `pk BETWEEN a AND b`. The grammar (`db::id_filter::parse_ids`) is comma-separated tokens, each
    a number or `a-b` (a reversed range is swapped); a number is ASCII digits only (no sign, `007` is 7) and
    must fit in `u64`. Anything else, such as `+5`, `1-2-3` or an overflowing number, is not an id filter
    and goes through as SQL.
  - Any other text is inserted **verbatim** as a WHERE clause. It must be a single statement; `1; DROP …` is
    rejected.
  - The filter is wrapped as `WHERE (<filter>\n)`: the newline ends a trailing `#` or `-- ` comment, so it
    can't comment out `ORDER BY` or `LIMIT`.
  - A filter that ends inside a quote or a `/* */` comment is rejected as `invalid_input`
    (`lexer::is_terminated`). A filter of only whitespace and comments counts as no filter (`lexer::is_blank`).
  - `browse_sql` returns the applied filter (`kind` is `FilterKind`: `ids` or `sql`) next to the SQL, and
    `browse` hands it to the UI so it can show how the text was read.
- **New query:** `table_select_sql` returns `browse::select_all_sql` (`SELECT *`, the quoted
  `Dialect::table_ref`, `LIMIT 100`) for the Sidebar's "New query: SELECT …".

Introspection uses `information_schema`:
- `SCHEMATA` for databases;
- `TABLES` for row estimates, sizes and views;
- `COLUMNS` and `STATISTICS` for structure and indexes;
- `KEY_COLUMN_USAGE` for foreign keys (`TableStructure.foreignKeys`, `ForeignKey { name, columns, refTable,
  refColumns }`, columns in key order). Only keys into the same database are read
  (`REFERENCED_TABLE_SCHEMA = TABLE_SCHEMA`); WordPress declares none, so they appear on plugin and custom
  tables only;
- `SHOW CREATE TABLE` for the DDL (`TableStructure.ddl` is `null` when it returns no row; the UI then shows
  no DDL block). `structure` runs the columns, indexes, foreign keys and DDL queries concurrently;
- `COLUMNS` again for editor autocompletion.

`Database::keys(database)` is the bulk loader: one `STATISTICS` and one `KEY_COLUMN_USAGE` query for the whole
database return `TableKeys { primaryKey, indexes, foreignKeys }` per table (the primary key is the `PRIMARY`
index, so it needs no query of its own). A table without any index or foreign key is absent.

## Schema cache

`src-tauri/src/schema.rs` keeps what only DDL changes, per session (`SchemaCache` in `Session.ext`) and per
database:

- `schema::database(session, db) -> DatabaseSchema`: `tables` (`Database::tables`; row estimates and sizes as
  of the load), `keys` (`Database::keys`, an empty `TableKeys` for a table without keys), `columns` (column
  names per table, `Database::completion`) and `incoming`, the reverse index of foreign keys: referenced
  table → `IncomingKey { table, foreignKey }`. Four queries (`tables`, `completion` and the bulk loader's
  two), run concurrently, however many tables.
- `schema::structure(session, db, table)`: one table's `TableStructure`, loaded on first use.
- `schema::fresh_structure`: reads the structure from the server and stores it. `describe_table` uses it
  because the DDL text's `AUTO_INCREMENT` moves with every insert; `list_tables` is not cached either,
  because row estimates and sizes move with the data.
- `schema::refresh(session)` (command `refresh_schema`, sent by the page's `view.refresh` before it reloads)
  drops everything.

Each entry is stamped with `Session::ddl_writes()` and the cache's refresh count. `Session::execute` bumps
`ddl_writes` after every user script that `classify::changes_schema` flags, also when it failed (a script may
stop after its DDL), and any stamp change makes every database's entry stale. The counter is session-wide
because a statement can name any database. A load stamped before DDL or a refresh finished is returned but
not kept. Applied plans never contain DDL and leave the cache alone. DDL made outside the app (another
client, WP-CLI) shows after `view.refresh`.

Users: `address::resolve` (table names, primary keys, filter columns), the lens
([References and kinds](#references-and-kinds)), documents, palette search and the rules layer
(applied presets, the Project page, the checks in `project_update`).

## References and kinds

`src-tauri/src/lens/` reads what a column holds and which references a table has (the `Lens` trait;
[architecture](architecture.md#values-and-the-lens)). `SchemaLens` answers from `schema::database`, so a
browse page or `table_references` call needs no structure query per table. `rules::lens` picks the lens of a
database and `rules::annotate` fills browse pages and query results with it.

**Declared references** are the foreign keys (`ReferenceSource::ForeignKey { constraint }`).

**Same-name inference** (`SchemaLens::inferred_targets`). Column `A.c` points at table `B` when:
- `B` has a single-column primary key named `c` (compared case-insensitively, so `comment_id` matches
  `wp_comments.comment_ID`);
- `A` is not `B`;
- `c` is not `A`'s own single-column primary key (a member of a composite key may still be a reference,
  like `wp_term_relationships.term_taxonomy_id`);
- `c` is not `id` (any case, so `ID` too);
- no declared foreign key of `A` contains `c`.

Several matching tables give several targets, all listed. The source is `SameName { column, indexed }`;
`indexed` says `A` has an index whose first column is `c`. Example: `wp_actionscheduler_logs.action_id` →
`wp_actionscheduler_actions`.

**Column kinds** (`ColumnKind`, in column order in `ResultSet.kinds`):
- a single-column primary key is `PrimaryKey { table, columns }`;
- a single-column foreign key, else an inferred reference, is `Reference { targets, by }` (each
  `ReferenceTarget` has the target table's address, its column and whether that column is its key).
  Composite foreign keys only appear in `table_references`;
- other members of a composite primary key are `PrimaryKey`;
- otherwise the SQL type decides: numeric `Number`, temporal `Timestamp` (zone `null`), everything else
  `Text`.

With rules ([presets.md](presets.md#effective-rules)), `RulesLens` adds project and preset references
(`Reference` with `by` `project` or `preset`) and kind rules (`Money { currency }`, `Timestamp { zone }`,
`Enum`, `Secret`, `Number`). Cells whose row decides their kind (conditional references, meta-key kinds) are
listed in `ResultSet.cell_kinds`; cells a value style colors in `ResultSet.styles`.

The cell decides the rest when the page builds a `Value`: NULL is `null` (`noReference` in a reference
column, as is `0`), `''` is `empty`, binary is `binary`, and a text cell listed in `ResultSet.serialized`
(PHP-serialized or JSON that parses, found with the full text of long cells) is `serialized` with a
preview such as `a:3:{…}`. A `secret` column masks every non-empty cell; a timestamp carries `utc` (Unix
seconds) when its zone is stored and the value is a time; a money cell carries its currency as a sourced
value.

**Where kinds are filled:**
- `Session::browse` loads the page and the schema concurrently, then sets the lens kinds (the rules lens
  when `RuleSources` are given and the rules change anything), the per-cell kinds and styles, and
  `BrowsePage.keys` (one `RowKey` per row; empty when the table has no primary key or a key cell is
  binary). If the schema fails to load, the page shows with type kinds.
- `Session::execute` sets type kinds, then, for every column whose `ColumnMeta.origin` names a table (the
  protocol's `org_table` and `org_name`, which sqlx exposes as `Column::origin`; also through an alias),
  the kind that table's lens gives, and per-cell kinds and styles for the columns of each origin table.
  Expressions keep their type kind. Query results have no provenance.
- `fetch` results (documents, the readout, change planning) carry no kinds until their caller annotates them.

`table_references(sessionId, database, table)` returns `{ outgoing, incoming }`, each a
`ResolvedReference { from, to, by }` with the columns of both ends, rule references (conditional ones too)
included and disabled or replaced inferences left out.

## Value conditions

`value_condition(sessionId, database, provenance, raw)` (`value/condition.rs`) turns a stored value into
the SQL condition the `Y` value command copies. `condition_sql` renders `ColumnEquals` matches with
`db::browse::equals_sql`, so quoting follows the session's `Dialect` (`NO_BACKSLASH_ESCAPES` included) and
`raw: None` is `IS NULL`. On the value column of a `[[meta]]` table of the effective rules, the row's key is
read first (`SELECT <key> … WHERE <row key> LIMIT 1` with the query timeout), giving
`` `meta_key` = '…' AND `meta_value` = '…' ``; a meta key column itself is a plain column condition.

## Value counts

`value_counts(sessionId, database, table, column, equals, limit)` (`value_counts.rs`, the grid's "Value
counts") reads `SELECT <column>, COUNT(*) FROM <table> [WHERE <equals>] GROUP BY <column> ORDER BY COUNT(*)
DESC, <column> LIMIT <limit>` on the metadata pool, with the query timeout capped at 5 s (`QUERY_TIMEOUT`).
The table and every named column must be in the schema cache (`invalid_input` otherwise); `equals` renders
with `equals_sql`. Each value is a `Value` with the column's kind from the lens and no provenance (a group has
no row); `limit` is clamped to 1..`MAX_LIMIT` (500).

## Narrowing a query

A query tab's value counts add their pick to the tab's SQL (`db/narrow.rs`). `refusal(script)` says why a
script cannot take one more condition, `narrowed(script, condition)` adds it. Only a script of exactly one
statement that starts with `SELECT` and reads one table qualifies: no `UNION`/`EXCEPT`/`INTERSECT`, `JOIN`,
comma join, subquery in `FROM`, `GROUP BY`, `HAVING`, `WINDOW` or `INTO` at the top level. An existing
`WHERE` becomes `(<where>) AND <condition>`; without one, `WHERE <condition>` goes after the table, before
`ORDER BY`, `LIMIT`, `FOR`, `LOCK` or `PROCEDURE`. Comments and the rest of the text stay as written.
`narrow_query(sessionId, sql, column, value)` renders `column = value` (`IS NULL` for none) with the
session's dialect and returns the narrowed SQL, or `invalid_input` with the refusal.

## Editing rows

Grid edits are core (`db/changes.rs`). Every write goes plan, review,
apply by id, undo by batch; the page never sends SQL to execute ([ADR 0012](adr/0012-plans-applied-by-id.md)):

1. `TableTab` queues a `ChangeRequest` (`queueChange` in the store): `setCell`, `deleteRow`, `duplicateRow`,
   `insertRow` or `addRow`. Documents use the same requests on a value's provenance row (see
   [architecture.md](architecture.md#documents)).
2. `plan_change` plans it (`db::changes::plan`): each variant fetches what it needs (the current row, the
   structure), then a pure `build_*` function turns that into a `Plan { database, description, statements,
   undo }`. A `setCell`, `deleteRow` or `duplicateRow` requires the key to match **exactly one row**. For
   `setCell` the undo restores the old value; for `deleteRow` it reinserts the row without its generated
   columns (and the description warns when a zero date would be reinserted, which fails under `NO_ZERO_DATE`).
   A row with binary cells can't be reinserted, because only a preview of its bytes is known, so it has no
   undo. `duplicateRow` copies every column except auto-increment and generated ones. `insertRow` has no
   undo; `addRow` (a row whose `keyColumn` the server assigns, such as a new meta key) undoes by deleting the
   newest row that holds exactly the inserted values (`<=>`, ordered by `keyColumn` descending, `LIMIT 1`),
   since the new key is only known after the insert.
   Rule actions ([presets.md](presets.md#action)) are planned without a `ChangeRequest`, by
   `plan_update` and `plan_delete` in `db/where_changes.rs`, which the page cannot reach. They select rows
   by an SQL condition, an update in several tables at once (`RowsUpdate { table, key_columns, filter, set }`,
   `set` as SQL expressions). Planning reads the matched rows (their key and set columns, or every column for a delete),
   at most 10,000, returns them with the plan (an action preview's old values), and refuses no match at all; the statements address exactly those rows by primary key,
   so a condition may read the table it changes. Undo restores each row's set columns by key, last step
   first, or re-inserts the deleted rows; a binary value means no undo. Updating a key column is refused.
3. The plan is stored in the session (`Session.plans`, `db/plans.rs`) under an id; the page receives a
   `PlannedChange { id, description, statements, canUndo }`, where `statements` are for display only.
   `discard_change` drops one. The pending tray lists each plan's description and statements as returned.
4. `apply_changes(ids)` takes the plans (atomically: an unknown id fails and leaves every plan pending) and
   runs their statements in **one transaction** on a pool connection (`Database::apply`). This is atomic only
   for transactional engines; MyISAM tables and implicit DDL commits are not covered. On failure the plans
   stay pending for a retry; on success it returns `{ batchId, affected, canUndo }`.
5. `undo_batch(batchId)` replays the batch's undo statements, last plan first. The last 20 applied batches
   are kept; a batch with a change that has no undo can't be undone. Plans and undo history die with the
   session.

The UI offers editing only for tables with a single-column `PRIMARY` key. Applying doesn't run the
destructive-statement check, because every statement came out of a plan the backend built and the user
reviewed.

## Adding another database engine

Callers only know `&dyn Database` and the `Dialect` it returns, so an engine is one folder and one factory
arm; nothing in `rules/` changes:

1. **Driver.** `src-tauri/src/db/<engine>/mod.rs` implementing `Database`.
2. **Dialect.** `Dialect` only knows MySQL (`backslash_escapes`); it needs an engine field and per-engine
   versions of: `quote_ident` and `table_ref` (backticks); `sql_string` and `like` (backslash escaping unless
   `NO_BACKSLASH_ESCAPES`); `system_schemas`. Outside the dialect: the lexer (`#` comments, backslash
   escapes), the cancel SQL (`KILL QUERY` → `pg_cancel_backend`; SQLite uses an interrupt), `DELETE … LIMIT 1`,
   and `UTC_TIMESTAMP()` and `INTERVAL` in preset SQL.
3. **Connection spec.** A `kind` (and a file `path`) on `ConnectionSpec`, the URL schemes in `cli.rs`, the
   keychain account format, and one arm in the app's `db::Connector` (`MySqlConnector` today).
4. **Cell decoding.** A `decode` per engine that maps into the same `Cell` enum.
5. **Introspection.** Postgres: `information_schema` plus `pg_catalog`. SQLite: `sqlite_master` and
   `pragma_table_info` / `pragma_index_list`. Make `ColumnInfo` engine-neutral, replacing `key = "PRI"`,
   `extra = "auto_increment"` and the `"PRIMARY"` index name that the UI checks.
6. **Multi-statement execution.** `fetch_many` relies on MySQL returning one result per statement. A portable
   approach splits with the lexer and runs statements one by one.
7. **Tests.** A sqlx feature, plus a Docker service or a temp-file fixture.

The roadmap lists SQLite as the next driver.

## Statement counts

`src-tauri/src/statement_counts.rs` measures what the object view, the palette, the readout and the
Project page send to the server on `wp_test` with the built-in presets, on every fixture server. Its
`#[ignore]` tests wrap the session's database in `test_support::Counting`, load the schema first (as a
session does once per database: `tables`, `keys` and `completion`, four statements) and assert the number
of `Database` calls each command makes afterwards. Each call is one round trip; a saved query's `USE` rides
along with its `SELECT`. Unit tests count with `FakeDatabase::queries()`.

| Command | Measured on | Before | After |
|---|---|---|---|
| `document` | order 4711 (`wp_wc_orders`) | 18 | 18 |
| `peek` | order 4711 | 1 | 1 |
| `palette_search` | `#4711` | 35 | 43 |
| `palette_search` | `=sandbox` | 28 | 16 |
| `readout` | `wp_test` | 19 | 20 |

The readout's count is without watched values (the measured project has none). Watched values add one
statement per table they name, however many watches it has (`readout/watched.rs`, `lines`): a `SELECT *`
whose `WHERE` ORs the condition of each watched row. Watching a value runs one more statement once, when it
is added and the table has a unique key besides the primary key (`identity`).
| `reference_coverage` | each of the 40 references of the Project page | 43 | 43 |

The last change gave every row one summary (architecture.md, "Summaries"). `#4711` reads the rows of its
`rows that reference` section again by key, one query per table (8): group rows hold only the group's
columns, and the hit must show the summary the row's own document shows. The readout reads the meta keys of
the summaries in its created timeline in one query per meta table (`wp_postmeta` for the products and the
order post). Peeks read meta keys with one `IN` query per meta table instead of the capped meta page, so a
peek of a posts-shaped order needs 2 statements, not 2 to 4.

What was batched earlier: the find sources of one table (`search/find.rs`) and the incoming reference groups from
one source table (`document/incoming.rs`) run as one `UNION ALL` of the per-filter `SELECT`s, each tagged
with `__databee_part` and ordered by it and the key (`Ctx::rows_per_filter`, `Ctx::capped_each`); a capped
group still counts on its own. Order 4711's document has no two incoming groups on one table, so its count
stays; `#4711` saves two (the synced post's `wp_postmeta` and `wp_wc_order_product_lookup` groups).

What stays one query each, and why:
- `exists` already runs one query per target table. One statement over several tables would need every key
  cast to text, which changes the outcome for binary keys and fails on mixed collations.
- Readout parts, anomalies and value pins are preset or saved SQL of their own: batched, one broken
  statement would fail every line instead of its own.
- `reference_coverage` is one call per reference from the Project page: a count, plus the sample of
  unresolved values only when some are unresolved (3 of the 40).
- `#4711` reads the row by key in each of the 11 entity tables with a single-column key, then the
  references of each found row; rows found in two tables repeat some identical group queries, whose hits
  the palette drops as duplicates.

## Integration tests

The integration tests are `src-tauri/tests/mysql.rs`, `changes.rs`, `sql_mode.rs`, the `server_tests.rs` of the modules and `src/statement_counts.rs` (see [testing.md](testing.md)).
Each test runs against every server in `DATABEE_TEST_SERVERS`. The default is the two Docker fixtures,
MariaDB 11.4 on `127.0.0.1:33306` and MySQL 8.4 on `127.0.0.1:33307` (user `root`, password `root`), so
driver behavior is checked on both engines.
