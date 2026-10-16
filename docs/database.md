# Database layer

`src-tauri/src/db/` is the MySQL/MariaDB driver and the SQL tooling around it. The commands that use it live
in `src-tauri/src/db_commands.rs`.

## Modules

| File | Responsibility |
|---|---|
| `db/mod.rs` | The `Database` trait every caller uses (`&dyn Database`, [ADR 0011](adr/0011-database-trait-object.md)), the `open` factory (one arm per engine) and re-exports |
| `db/types.rs` | Shared DTOs (`ResultSet` with its `id` and the IPC-skipped `full_texts`, `ExecuteResponse` with its `failure: StatementFailure`, `TableInfo`, `TableStructure`, `BrowseRequest`, `BrowsePage`, `AppliedFilter`, `FilterKind`) |
| `db/dialect.rs` | `Dialect { backslash_escapes }` (`Dialect::MYSQL`, or `from_sql_mode`): `quote_ident` (backticks), `sql_string`, `like` / `like_contains`, `table_ref`, `system_schemas`; `escape_like` |
| `db/browse.rs` | `browse_sql` (the table view's paged SELECT plus the filter it applied), `page_limit`, `MAX_PAGE_ROWS` |
| `db/mysql/mod.rs` | `MySqlSession`, the only driver, implementing `Database`: the metadata pool, the editor connection, `connect_options`. It owns all I/O. |
| `db/mysql/editor.rs` | `EditorConnection`: the editor connection's lock, thread id, current database, `reconnect`, `kill_query` and `kill_connection` |
| `db/mysql/kill.rs` | `Killer`: `KILL QUERY <id>` or `KILL <id>` on a one-off connection, for any server thread id; `spawn_kill_query` fires one in the background |
| `db/mysql/introspect.rs` | `databases`, `tables`, `structure`, `completion` over the pool; rows are read by column alias |
| `db/mysql/script.rs` | `run_script` (text protocol, one `ResultSet` per statement, column recovery; returns a `ScriptOutcome` of the finished results and the failing statement, `into_results` for callers that need all of it), `scalar`, `thread_id`, `read_dialect` |
| `db/mysql/timeouts.rs` | `connect_within`, `query_within`, and `pooled_within`, which reads the pool connection's thread id, kills its query on timeout and closes the connection |
| `db/fake.rs` | `FakeDatabase`, an in-memory `Database` for unit tests (canned results matched by SQL fragment, recorded `apply` batches); behind `cfg(any(test, feature = "test-util"))` |
| `db/lexer.rs` | A small MySQL-aware scanner, not a parser. `scan` is an iterator of borrowed `Piece { kind, text, start, depth }`. It knows the three quote styles, backslash escapes, the `#`, `-- ` and `/* */` comments, executable `/*! */` comments and `@` variables. `split_statements`, `words`, `is_terminated`, `is_blank` |
| `db/classify.rs` | What a statement is: `verb_index` (the deciding word, also after a `WITH` clause) and `returns_rows` |
| `db/safety.rs` | `destructive_reason` (which statements need confirmation) |
| `db/cell.rs` | `Cell`: decodes text-protocol bytes by column `Family`, computed once per result set from the type name; `CellLimit` cuts long text to a preview |
| `db/cell_cache.rs` | `CellCache`: numbers result sets per session and keeps the full text of their `LongText` cells for `fetch_cell` |
| `db/id_filter.rs` | Primary-key shorthand in the WHERE field: `3229`, `3229, 3227`, `3220-3229` |

Outside `db/`:
- `connection.rs` has `ConnectionSpec` and `Secret`.
- `session.rs` has `Session { id, spec, db: Box<dyn Database>, ext, plans, cells }` and the `Sessions` map. `ext` is a
  type-keyed `Extensions` map where a module keeps per-session data that dies with the session
  ([ADR 0013](adr/0013-session-extensions.md)); the WordPress lens keeps its `WpCache` there.
- `error.rs` maps `sqlx::Error` to `AppError`.

## Connection model

Every connection opens **two channels**:

- **The editor connection.** This is one `MySqlConnection` behind a `tokio::sync::Mutex`. Every user query and
  `USE` runs on it, so session state survives between runs: `USE`, `@vars`, `SET`, open transactions and
  temporary tables all persist. Its server thread id comes from `CONNECTION_ID()` and is kept for cancelling.
- **The metadata pool.** This is a `MySqlPool` of 3 connections. It serves schema introspection, table
  browsing, WordPress reads and applying planned changes. Pool connections don't share the editor's
  `USE`, so these queries always use qualified `` `db`.`table` `` names.

`connect` opens the pool and the editor connection concurrently (`tokio::try_join!`) under one
`connect_timeout`. The editor connection has no statement cache (`statement_cache_capacity(0)`), so the
statements that column recovery prepares are closed right away instead of staying open on the server.

Other details:
- **Connect options** are built from `ConnectionSpec`: socket or host and port, user and optional password
  (`connect_options`). The database is added per connection (`with_database`): the spec's database for the
  pool and the first editor connection, the current database on reconnect, none for `KILL`. Server defaults are left alone on purpose: sqlx is told not to change `sql_mode`, `time_zone` or
  `PIPES_AS_CONCAT`. Instead the editor connection reads `@@SESSION.sql_mode` on connect and reconnect, and
  `Database::dialect` reports whether backslashes escape (`NO_BACKSLASH_ESCAPES`); browse and every
  WordPress literal and identifier use it ([ADR 0018](adr/0018-escaping-follows-sql-mode.md)). A user's own
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
`CellLimit::NONE`, because its callers (WordPress reads, change planning) need every value in full.

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
  `openRow` and `openInDatabase` ([frontend.md](frontend.md#opening-rows)). `browse::columns_filter` renders each as
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
- `SHOW CREATE TABLE` for the DDL (`TableStructure.ddl` is `null` when it returns no row; the UI then shows
  no DDL block). `structure` runs these three queries concurrently;
- `COLUMNS` again for editor autocompletion.

## Editing rows

Grid edits are core (`db/changes.rs`) and work without the WordPress module. Every write goes plan, review,
apply by id, undo by batch; the page never sends SQL to execute ([ADR 0012](adr/0012-plans-applied-by-id.md)):

1. `TableTab` queues a `ChangeRequest` (`queueChange` in the store): `setCell`, `deleteRow`, `duplicateRow` or `insertRow`.
2. `plan_change` plans it (`db::changes::plan`): each variant fetches what it needs (the current row, the
   structure), then a pure `build_*` function turns that into a `Plan { database, description, statements,
   undo }`. A `setCell`, `deleteRow` or `duplicateRow` requires the key to match **exactly one row**. For
   `setCell` the undo restores the old value; for `deleteRow` it reinserts the row without its generated
   columns (and the description warns when a zero date would be reinserted, which fails under `NO_ZERO_DATE`).
   A row with binary cells can't be reinserted, because only a preview of its bytes is known, so it has no
   undo. `duplicateRow` copies every column except auto-increment and generated ones.
3. The plan is stored in the session (`Session.plans`, `db/plans.rs`) under an id; the page receives a
   `PlannedChange { id, description, statements, canUndo }`, where `statements` are for display only.
   `discard_change` drops one. The pending tray lists each plan's description and statements as returned.
4. `apply_changes(ids)` takes the plans (atomically: an unknown id fails and leaves every plan pending) and
   runs their statements in **one transaction** on a pool connection (`Database::apply`). This is atomic only
   for transactional engines; MyISAM tables and implicit DDL commits are not covered. On failure the plans
   stay pending for a retry; on success it returns `{ batchId, affected, canUndo }` and bumps the session's
   write counter of each written database (`Session::writes`), which caches compare to notice changed data.
5. `undo_batch(batchId)` replays the batch's undo statements, last plan first. The last 20 applied batches
   are kept; a batch with a change that has no undo can't be undone. Plans and undo history die with the
   session.

The UI offers editing only for tables with a single-column `PRIMARY` key. Applying doesn't run the
destructive-statement check, because every statement came out of a plan the backend built and the user
reviewed.

## Adding another database engine

Callers only know `&dyn Database` and the `Dialect` it returns, so an engine is one folder and one factory
arm; nothing in `wp/` changes:

1. **Driver.** `src-tauri/src/db/<engine>/mod.rs` implementing `Database`.
2. **Dialect.** An `Engine` variant with its `Dialect`. These are MySQL-specific today and need a per-engine
   version: `quote_ident` and `table_ref` (backticks); `sql_string` and `like` (backslash escaping unless
   `NO_BACKSLASH_ESCAPES`); `system_schemas`. Outside the dialect: the lexer (`#` comments, backslash
   escapes), the cancel SQL (`KILL QUERY` → `pg_cancel_backend`; SQLite uses an interrupt), `DELETE … LIMIT 1`,
   and `UTC_TIMESTAMP()` and `INTERVAL` in the WordPress plans.
3. **Connection spec.** A `kind` (and a file `path`) on `ConnectionSpec`, the URL schemes in `cli.rs`, the
   keychain account format, and one arm in `db::open`.
4. **Cell decoding.** A `decode` per engine that maps into the same `Cell` enum.
5. **Introspection.** Postgres: `information_schema` plus `pg_catalog`. SQLite: `sqlite_master` and
   `pragma_table_info` / `pragma_index_list`. Make `ColumnInfo` engine-neutral, replacing `key = "PRI"`,
   `extra = "auto_increment"` and the `"PRIMARY"` index name that the UI checks.
6. **Multi-statement execution.** `fetch_many` relies on MySQL returning one result per statement. A portable
   approach splits with the lexer and runs statements one by one.
7. **Tests.** A sqlx feature, plus a Docker service or a temp-file fixture.

The roadmap lists SQLite as the next driver.

## Integration tests

The integration tests are `src-tauri/tests/mysql.rs` and the WordPress tests (see [testing.md](testing.md)).
Each test runs against every server in `DATABEE_TEST_SERVERS`. The default is the two Docker fixtures,
MariaDB 11.4 on `127.0.0.1:33306` and MySQL 8.4 on `127.0.0.1:33307` (user `root`, password `root`), so
driver behavior is checked on both engines.
