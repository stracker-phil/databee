# Database layer

`src-tauri/src/db/` is the MySQL/MariaDB driver and the SQL tooling around it. The commands that use it live
in `src-tauri/src/db_commands.rs`.

## Modules

| File | Responsibility |
|---|---|
| `db/mod.rs` | Module declarations, re-exports, and `quote_ident`, a wrapper around `Dialect::MYSQL` |
| `db/types.rs` | Shared DTOs (`ResultSet`, `ExecuteResponse`, `TableInfo`, `TableStructure`, `BrowseRequest`, `BrowsePage`, `AppliedFilter`, `FilterKind`) |
| `db/dialect.rs` | `Dialect` (only `Dialect::MYSQL` so far): `quote_ident` (backticks), `sql_string`, `table_ref`, `system_schemas` |
| `db/browse.rs` | `browse_sql` (the table view's paged SELECT plus the filter it applied), `page_limit`, `MAX_PAGE_ROWS` |
| `db/mysql.rs` | `MySqlSession`, the only driver. It owns all I/O. |
| `db/lexer.rs` | A small MySQL-aware scanner, not a parser. It knows the three quote styles, backslash escapes and the `#`, `-- ` and `/* */` comments. `split_statements`, `words`, `is_terminated`, `is_blank` |
| `db/safety.rs` | `destructive_reason` (which statements need confirmation), `returns_rows` |
| `db/cell.rs` | `Cell`: decodes text-protocol bytes by column type |
| `db/id_filter.rs` | Primary-key shorthand in the WHERE field: `3229`, `3229, 3227`, `3220-3229` |

Outside `db/`:
- `connection.rs` has `ConnectionSpec` and `Secret`.
- `session.rs` has `Session { id, spec, db: MySqlSession }` and the `Sessions` map.
- `error.rs` maps `sqlx::Error` to `AppError`.

## Connection model

Every connection opens **two channels**:

- **The editor connection.** This is one `MySqlConnection` behind a `tokio::sync::Mutex`. Every user query and
  `USE` runs on it, so session state survives between runs: `USE`, `@vars`, `SET`, open transactions and
  temporary tables all persist. Its server thread id comes from `CONNECTION_ID()` and is kept for cancelling.
- **The metadata pool.** This is a `MySqlPool` of 3 connections. It serves schema introspection, table
  browsing, WordPress reads, applying planned changes and `KILL`. Pool connections don't share the editor's
  `USE`, so these queries always use qualified `` `db`.`table` `` names.

Other details:
- **Connect options** are built from `ConnectionSpec`: socket or host and port, user, optional password and
  database. Server defaults are left alone on purpose: sqlx is told not to change `sql_mode`, `time_zone` or
  `PIPES_AS_CONCAT`.
- **Timeouts:**
  - `connect_timeout_secs` covers connecting.
  - `query_timeout_secs` covers `execute`, `browse` and `fetch`. When an `execute` times out, DataBee kills the
    whole editor connection, reconnects it, and returns `timeout`.
- **Cancel** runs `KILL QUERY <editor thread id>` on the pool. Cancel doesn't need the editor lock, which the
  running query holds. A cancelled query on MariaDB returns error 1317, which maps to `cancelled`. MySQL
  instead reports an interrupted `SLEEP` as a result row.
- **Reconnects:** when `execute` fails with a connection error, the editor connection is rebuilt with the
  current database. The query itself is not retried.
- **TLS:** none. sqlx is compiled without a TLS backend. `mysql-rsa` handles `caching_sha2_password` over
  plaintext connections. To add TLS:
  1. Enable `tls-rustls` in `Cargo.toml`.
  2. Add `ssl_mode`/CA fields to `ConnectionSpec`.
  3. Set them in `connect_options`.

## Running a query

1. **Confirmation.** `db_commands::execute` checks `destructive_reason`. It runs only when the
   `confirm_destructive` setting is on and the call isn't already confirmed. A flagged statement returns
   `ConfirmationRequired(reason)`.
2. **Safety rules** (`safety.rs`) look at each statement's bare upper-cased words. Text inside quotes doesn't
   count. Flagged:
   - `DROP`
   - `TRUNCATE`
   - `DELETE` or `UPDATE` without a `WHERE`
   - `ALTER … DROP`

   Only the first word picks the rule, so `WITH … DELETE` and `REPLACE` are not flagged.
3. **Execution.** `run_script` sends the **whole script as one multi-statement text-protocol call**
   (`sqlx::raw_sql(..).fetch_many`). Each statement result closes one `ResultSet`, with columns, rows,
   `affectedRows` and `lastInsertId`.
4. **Row limit.** Rows beyond `row_limit` are counted but not decoded: `totalRows` stays exact and
   `truncated` is set. The query editor has no paging.
5. **Empty results.** The text protocol sends no column metadata for an empty result. When the lexer's split
   matches the result count, row-returning statements are `PREPARE`d to recover their column names.
6. **History.** A history entry is written for every run, including failed ones.
7. **Current database.** `SELECT DATABASE()` refreshes the session's current database after every run. The
   UI follows a `USE` this way.

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

## Browsing tables

`browse_table` → `MySqlSession::browse` → `browse_sql`:

```sql
SELECT * FROM `db`.`t` [WHERE (<filter>\n)] [ORDER BY `col` ASC|DESC] LIMIT <limit + 1> OFFSET <offset>
```

- **Paging:** fetching one extra row tells whether there is a next page. The UI uses
  `min(rowLimit, 500)` per page; `page_limit` clamps the requested limit to `1..=MAX_PAGE_ROWS` (500).
- **Sorting:** one column at a time, quoted with `Dialect::quote_ident`.
- **Filter:**
  - If the table has a single-column primary key and the text matches the id grammar, the text becomes
    `pk = n`, `pk IN (…)` or `pk BETWEEN a AND b`.
  - Any other text is inserted **verbatim** as a WHERE clause. It must be a single statement; `1; DROP …` is
    rejected.
  - The filter is wrapped as `WHERE (<filter>\n)`: the newline ends a trailing `#` or `-- ` comment, so it
    can't comment out `ORDER BY` or `LIMIT`.
  - A filter that ends inside a quote or a `/* */` comment is rejected as `invalid_input`
    (`lexer::is_terminated`). A filter of only whitespace and comments counts as no filter (`lexer::is_blank`).
  - `browse_sql` returns the applied filter (`kind` is `FilterKind`: `ids` or `sql`) next to the SQL, and
    `browse` hands it to the UI so it can show how the text was read.

Introspection uses `information_schema`:
- `SCHEMATA` for databases;
- `TABLES` for row estimates, sizes and views;
- `COLUMNS` and `STATISTICS` for structure and indexes;
- `SHOW CREATE TABLE` for the DDL;
- `COLUMNS` again for editor autocompletion.

## Editing rows

Grid edits use the change pipeline in `wp/changes.rs`, which serves Database mode too:

1. `TableTab` queues a `ChangeRequest`: `setCell`, `deleteRow`, `duplicateRow` or `insertRow`.
2. `plan_change` reads the current row. A `setCell` or `deleteRow` requires the key to match **exactly one
   row**. It returns `PlannedChange { description, statements, undo }`. For `setCell` the undo restores the
   old value; for `deleteRow` it reinserts the row. A row with binary cells can't be reinserted, because only
   a preview of its bytes is known, so it has no undo.
3. The pending tray shows the SQL. Applying sends every pending statement of the session to `apply_changes`,
   which runs them in **one transaction** on a pool connection (`MySqlSession::apply`). This is atomic only
   for transactional engines; MyISAM tables and implicit DDL commits are not covered.
4. Undo replays the `undo` statements in reverse order. The last 20 applied batches are kept.

The UI offers editing only for tables with a single-column `PRIMARY` key. `apply_changes` doesn't run the
destructive-statement check, because every statement it receives came out of a plan the user reviewed.

## Adding another database engine

There is no driver trait yet. `Session.db` is a concrete `MySqlSession`, and every `wp/*` function takes
`&MySqlSession`. A second engine (SQLite or Postgres) needs the following:

1. **Driver abstraction.** Add an enum `Driver { MySql(MySqlSession), Sqlite(…) }`, or a trait with the
   current public API: `connect`, `close`, `execute`, `cancel`, `use_database`, `databases`, `tables`,
   `structure`, `completion`, `browse`, `apply`, `fetch`, `current_database`, `server_version`. An enum is
   easier with async. Switch `Session.db`, `db_commands.rs` and `wp/*` to it.
2. **Dialect.** `db/dialect.rs` holds the start of a per-engine `Dialect`; only `Dialect::MYSQL` exists, and
   `db::quote_ident` and `wp::sql_string` still call it directly. These are MySQL-specific today and need a
   per-engine version:
   - `Dialect::quote_ident` and `Dialect::table_ref` (backticks);
   - `Dialect::sql_string` (backslash escaping);
   - the lexer options (`#` comments, backslash escapes);
   - `Dialect::system_schemas`;
   - the cancel SQL (`KILL QUERY` → `pg_cancel_backend`; SQLite uses an interrupt);
   - `DELETE … LIMIT 1`;
   - `UTC_TIMESTAMP()` and `INTERVAL` in the WordPress plans.
3. **Connection spec.** Add a `kind` and a file `path` to `ConnectionSpec`. Extend the URL schemes in
   `cli.rs`, and decide the keychain account format.
4. **Cell decoding.** Add a `decode` per engine that maps into the same `Cell` enum.
5. **Introspection.** Postgres: `information_schema` plus `pg_catalog`. SQLite: `sqlite_master` and
   `pragma_table_info` / `pragma_index_list`. Make `ColumnInfo` engine-neutral, replacing `key = "PRI"`,
   `extra = "auto_increment"` and the `"PRIMARY"` index name that the UI checks.
6. **Multi-statement execution.** `fetch_many` relies on MySQL returning one result per statement. A portable
   approach splits with the lexer and runs statements one by one.
7. **Tests.** Add a sqlx feature, plus a Docker service or a temp-file fixture.

The roadmap lists SQLite as the next driver.

## Integration tests

The integration tests are `src-tauri/tests/mysql.rs` and the WordPress tests (see [testing.md](testing.md)).
Each test runs against every server in `DATABEE_TEST_SERVERS`. The default is the two Docker fixtures,
MariaDB 11.4 on `127.0.0.1:33306` and MySQL 8.4 on `127.0.0.1:33307` (user `root`, password `root`), so
driver behavior is checked on both engines.
