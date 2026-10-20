# 0027. Table live check by per-row checksums

## Status

Accepted (2026-10-20), implemented.

## Context

A developer keeps a table tab open while a plugin, WP-CLI or a test writes to the same rows. The page then
shows stale values with no hint, and a refresh shows nothing about what moved.
The document live mode (`src-tauri/src/document/watch.rs`) watches one object and keeps its last document
in `Session.ext`; a page of up to 500 rows needs something cheaper. The database must not be touched:
triggers or audit tables write to the user's database, and the binary log is often off and needs
privileges.

## Decision

The active table tab polls the rows it shows by primary key (`src-tauri/src/table_check.rs`,
`src/lib/tableCheck.ts`, `src/components/TableCheck.tsx`):

- **Checksums**: one statement per tick reads one checksum per shown row,
  `MD5(CONCAT(IFNULL(MD5(col),'-'), …))` over the page's columns, `WHERE <key> IN (…)`. The per-column MD5
  and the `-` for NULL keep rows that differ in any value (NULL and `''` included) apart.
- **Baseline in the page**: the first check after a page loads is the baseline. A row with another
  checksum is changed, a row with none is removed. Only changed rows not yet read at their current
  checksum are read in full (`table_check_rows`, one more statement), compared with the shown cells to name
  the changed columns.
- **Bounds**: every read is capped by `CHECK_TIMEOUT` (3 s); a check covers at most `MAX_ROWS` (500).
  The tab checks every `TABLE_CHECK_MS` (4 s) only while it is active, the window has the focus, no cell is
  being edited, no change to the table is pending and the connection is up.
- **Read-only**: nothing is written to the database; the backend keeps no state for it. The check never
  changes what the page shows, it marks rows.

Triggers or an audit table were rejected because they write to the user's database. Reading the binary
log was rejected for its privileges and because local servers often run without it. Reusing the
document live mode was rejected: it rebuilds whole documents and keeps them per session, too heavy for a
page of rows.

## Consequences

- Concurrent writes show on the page as marked rows and columns without a refresh.
- Tables without a primary key readable as text get no live check, and the tab says why.
- Inserted rows are not detected; only the shown rows are checked.
- Each tick costs one indexed query over at most 500 rows; changed rows cost one more.
