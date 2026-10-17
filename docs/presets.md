# Presets

A preset is a TOML file of rules that tells DataBee what the tables of an application mean: which columns
reference which rows, what kind of value a column or meta key holds, which values get a color, and which
queries, anomalies and actions belong to it. The code is generic (`src-tauri/src/rules/`); application
knowledge lives only in preset files. Rules follow [`.claude/rules/presets.md`](../.claude/rules/presets.md).

Status: presets are parsed, checked and listed (`presets_list`); projects are stored and edited
(`project_get`, `project_update`, `project_export`, `project_export_to`) and shown on the
[Project page](#the-project-page) (`project_rules`, `rule_draft`, `coverage_preview`); references, kinds and value styles apply to browse
pages, query results and `table_references` ([Effective rules](#effective-rules)), `rules_info` reports
the prefixes, presets and entities, and `reference_coverage` counts how many rows of a reference resolve
([Coverage](#coverage)). `document` builds a row's document from meta tables, references, entities,
patterns and kinds ([Documents](#documents)). The WordPress screens read entities, meta tables and kinds
through adapters ([wordpress.md](wordpress.md#screens-until-u12)); pinned queries, anomalies, actions and
find sources are parsed but not used yet.

## Where presets live

| Source | Location | Notes |
|---|---|---|
| Built-in | `src-tauri/presets/*.toml`, compiled in (`rules::catalog::BUILT_IN`) | `wordpress`, `woocommerce`, `action-scheduler` ([wordpress.md](wordpress.md)) |
| User | `~/Library/Application Support/com.philippstracker.databee/presets/*.toml` | reread when a file is added, removed or saved |

- A user preset whose `name` equals a built-in's **replaces** that built-in.
- Two user files with the same `name`: the second (by file name) is listed with an error and does not load.
- A file that does not parse is listed by its file stem with the error and does not apply. It does not
  replace a built-in, even when its file name matches.
- A preset whose `requires` names a missing or broken preset, or forms a cycle, is listed with an error and
  does not apply.

`presets_list()` returns `{ name, source: builtIn | user { path }, error }` for each.

## Format

Table names are written **without the prefix**; the engine adds the project's prefix, or the network prefix
for `network_tables`. Column references are `table.column`. Unknown sections and unknown keys are errors,
so typos never pass silently.

The same snake_case keys are used in projects (JSON) and in the TypeScript types (`types.gen.ts`), unlike
the camelCase of other IPC types: a rule is stored and exported verbatim.

Titles (`[[pinned]] title`, `[[anomaly]] text`) may use `{p}` and `{np}` and are checked like SQL.

```toml
[preset]
name = "shop"
requires = ["blog"]                 # applied first; must exist
marker_tables = ["orders"]          # the preset applies when these exist under the prefix
network_tables = ["users"]          # once per network, resolved under the network prefix ({np})
```

### `[[meta]]` key/value tables

```toml
[[meta]]
table = "order_meta"
owner = "orders"                    # owner and fk together, or neither (a standalone table)
fk = "order_id"
key = "meta_key"
value = "meta_value"
```

An owned table declares the reference `fk → owner`, to the owner's single-column primary key, with the id
`<table>.<fk>` (`order_meta.order_id`): it is a preset reference like any other, so `disabled` and project
overrides apply to it.

### `[[reference]]` virtual foreign keys

```toml
[[reference]]
from = "orders.customer_id"         # 0 or NULL means "no reference"
to = "users.id"

[[reference]]
from = "order_meta.meta_value"
when = { meta_key = "_customer" }   # on the source row
to = "users.id"

[[reference]]
id = "same_email"                   # optional
from = "orders.email"
when = { customer_id = "0" }
to = "orders.email"                 # any column: external ids, shared emails
to_when = { customer_id = "0" }     # on the target row
label = "same email"                # names the incoming group
columns = ["id", "status", "_qty"]  # the group's columns; keys of the source's meta table pivot in
```

**Conditions** (`when`, `to_when`, `owner_when`, an action's `when`) map a column to a match; all must hold.
They compare exactly (case-sensitive, whatever the column's collation); NULL matches nothing, not even
`not`. A regular expression that does not compile is a load error.

| Value | Means |
|---|---|
| `"x"` or `5` | equals |
| `["a", "b"]` | IN |
| `{ glob = "_wc_*" }` | glob (`*` matches any run of characters) |
| `{ like = "%x%" }` | SQL LIKE |
| `{ regexp = "^[0-9]+$" }` | regular expression |
| `{ not = <any of the above> }` | negation |

**Ids.** Without `id`, a reference's id is `<from>[?<when>]`, e.g. `order_meta.meta_value?meta_key=_customer`
(IN as `=[a,b]`, operators as `=glob(x)`, `=like(x)`, `=regexp(x)`, negation with `!`). When two rules of a
preset would get the same id (one column pointing at two tables), both get `-><to>` appended:
`items.order_id->orders.id`. Ids are unique per preset; a project disables a rule by its id.

### `[[entity]]`

```toml
[[entity]]
name = "order"
table = "posts"
filter = "post_type = 'order'"      # SQL; filtered entities win over unfiltered ones on a table
summary = ["post_status", "_total"] # header: columns or meta keys
created = "post_date_gmt"
search = ["post_title"]             # contains
search_exact = ["_txn_id"]          # exact; may name meta keys
people = true                       # found by the palette's `@`
```

### `[[kind]]`

Exactly one of `money`, `timestamp`, `enum = true`, `secret = true`, `number = true`, scoped to `columns`
(globs on both sides: `*.*_gmt`) and/or `meta_keys` (globs). `in` narrows `meta_keys` to some `[[meta]]`
tables; without it they match in every applied meta table.

```toml
[[kind]]
columns = ["orders.total"]
money = { currency = ["currency", "_currency"] }   # first non-empty: row column, owner column, sibling meta key
[[kind]]
meta_keys = ["_paid_at"]
in = ["order_meta"]
timestamp = "unix"                  # "unix" | "utc" | "local" (shown raw, never converted)
[[kind]]
columns = ["users.password"]
secret = true                       # masked until clicked
```

### `[[style]]` value styles

Scoped like kinds, never global. `match` is `"="`, `"LIKE"` or `"REGEXP"`; `color` one of the scheme's
style colors `red green yellow blue purple`. `enabled` defaults to `true`; preset styles ship with
`enabled = false` and a project switches them on.

```toml
[[style]]
columns = ["orders.status"]
match = "="
value = "failed"
color = "red"
enabled = false
```

### `[[pattern]]` groups matched by SQL

```toml
[[pattern]]
name = "log"                        # same name and owner: one group (OR)
owner = "orders"
owner_when = { type = "order" }
table = "log"
where = "message LIKE CONCAT('%', {meta._txn_id}, '%')"   # skipped when a placeholder is NULL or ''
order = "created"
limit = 100
```

On a row of `owner` whose `owner_when` holds, `where` selects rows of `table`. Rules with the same `name` and
`table` on one owner are one group, their conditions OR'ed. `{id}` is the row's key (a number literal when it
is one), `{owner.<column>}` a column of the row and `{meta.<key>}` the first value of the key in the row's
meta tables, both as SQL literals; a rule whose `{owner.…}` or `{meta.…}` value is NULL or `''` is skipped.
Rows are ordered by `order` (then the primary key); `limit` lowers the group's cap of 20 rows, and `order`
names the column whose timestamps go into `events`.

### `[[pinned]]` queries

```toml
[[pinned]]
id = "orders"
title = "{p}orders"
sql = "SELECT id, status FROM {p}orders ORDER BY id DESC"
home = { show = "rows", limit = 5 }  # or { show = "count" }, { show = "value" }; omit: palette only
```

### `[[anomaly]]`

Shown on the readout while `sql` returns a non-zero number.

```toml
[[anomaly]]
id = "failed"
sql = "SELECT COUNT(*) FROM {p}jobs WHERE status = 'failed'"
text = "{p}jobs status = failed"
open = { table = "jobs", where = "status = 'failed'" }   # or { pinned = "<id of this preset>" }
fix = "purge_failed"                # a scope = "table" action of this preset
```

### `[[action]]`

Exactly one of `update` (a list; several tables), `delete` or `sql`. Structured effects get undo from a
before-image; free `sql` has none and may have a `preview_sql`.

```toml
[[action]]
name = "set_status"                 # unique per table within the preset
table = "orders"
scope = "row"                       # default; "table" has no {id} and selects rows by `where`
when = { status = ["pending"] }
params = [{ name = "status", choices_sql = "SELECT DISTINCT status FROM {p}orders", free = true }]
update = [
  { set = { status = "{param.status}", updated = { sql = "UTC_TIMESTAMP()" } } },
  { table = "posts", key = "id", where = "type = 'order'", set = { status = "{param.status}" } },
]

[[action]]
name = "purge_failed"
table = "jobs"
scope = "table"
delete = { where = "status = 'failed'" }
```

Params: `name`, `choices_sql`, `free`, `type = "int"`, `default`, `optional`.

**Listing.** A document lists the row actions of its table whose `when` holds on the row
(`Document.actions`); `table_actions` lists the table actions. Actions on tables the database lacks are
dropped (`EffectiveRules.actions`).

**Planning** (`plan_action`, `document/actions/plan.rs`):
- Params are checked first: unknown names and missing required values (without `default`, not `optional`)
  are refused, an empty value counts as missing (`optional` makes it NULL), `type = "int"` must parse as an
  integer, and unless `free` the value must be one of the `choices_sql` rows.
- A row action is refused when its `when` no longer holds on the row.
- `update`: each step selects its rows by the row itself (the first step without `table` and `key`), or by
  `key` (default: the step table's primary key) `= {id}`, plus the action's `when` on the action's table and
  the step's `where`; a step on a missing table is skipped. A table action selects by `when` and `where`.
  The steps become one `UpdateWhere` request ([database.md](database.md#editing-rows)). A text `set` value
  that is a single placeholder is that placeholder's literal (NULL stays NULL); other text is one string
  literal with the placeholders filled in as text. `{ sql = "…" }` is an expression.
- `delete`: the same selection, as a `DeleteWhere` request.
- `sql`: run as written, without undo; the first value of `preview_sql` goes into the description.

### `[[find]]`

Extra sources for find everywhere, besides meta values and entity search columns.

```toml
[[find]]
table = "log"
columns = ["message"]
match = "contains"                  # default "exact"
```

## Placeholders

Preset SQL is trusted configuration in the MySQL/MariaDB dialect. Placeholders (`rules::placeholder`):

| Placeholder | Renders | Allowed in |
|---|---|---|
| `{p}`, `{np}` | the prefix, the network prefix, as text | all SQL and titles |
| `{id}` | the row's primary key, as a literal (a number when it parses as one) | patterns, row-scope actions |
| `{owner.<column>}`, `{meta.<key>}` | the owner row's value, as a literal | patterns |
| `{param.<name>}` | a declared param, as a literal (NULL when empty) | actions |

Braces with any other content are SQL and stay as written (`'^[0-9a-f]{32}$'`).

The metadata pool has no default database, so in SQL that documents and actions run (pattern `where`,
entity `filter`, action SQL, `choices_sql`, `preview_sql`) every `{p}` and `{np}` outside quotes is qualified
with the database (`placeholder::qualify_prefixes`, `Ctx::qualified`): `{p}options` reads and writes
`` `db`.wp_options `` whatever the connection's default database is. Inside quotes (`'{p}capabilities'`)
they stay plain text.

## Errors

Each rule is read on its own, so an error names the file, the rule and the field:

```
/…/presets/shop.toml: [[reference]] #3 (line 40): `when.meta_key`: unknown condition `glib`, expected glob, like, regexp or not
/…/presets/shop.toml: [[action]] #1 (line 88): `update[0].set.status`: `{param.state}` names no declared param
```

TOML syntax errors keep the parser's line and column.

## Projects

A project holds the rules a user adds for one database of one connection (`rules/project.rs`). Projects
live in `projects.json` in the config directory, keyed by `<connection slug>/<database>`; renaming a
connection's slug moves its keys. A database without a stored project gets an empty one.

| Field | Means |
|---|---|
| `connection`, `database` | the key's parts |
| `prefix` | the table prefix; `null`: suggested from the presets' marker tables |
| `network_prefix` | where `network_tables` live (multisite: one project per prefix, `wp_2_` with network prefix `wp_`); `null`: the shortest prefix that holds them, else the prefix |
| `presets` | enabled presets by name; `null`: every preset whose marker tables exist |
| `references`, `kinds`, `styles` | rules in the preset shape above |
| `disabled` | ids of preset or inferred rules switched off |
| `saved` | saved queries `{ id, title, sql, pin }`; `pin` has the shape of `[[pinned]] home` |

**Editing.** `project_update(session_id, database, change)` applies one `ProjectChange` (`op`):
`addReference`, `updateReference` and `removeReference` by id; `addKind`, `updateKind`, `removeKind`,
`addStyle`, `updateStyle`, `removeStyle` by index (these rules have no id); `disable` and `enable` an id;
`setPrefix`, `setNetworkPrefix`, `setPresets`. Before storing, every table a rule names must exist under the
prefix or the network prefix (the stored ones, else the suggestions), and every literal column (glob parts are
not checked), `when` and `to_when` column must exist in its table. A prefix must start some table name,
preset names must load. The error names the field: ``` `reference.when.meta_key`: `wp_posts` has no column
`meta_key` ```. A reference without `id` gets its default id; when the project already uses it, `-><to>` is
appended.

**Overrides.** A project reference with the same `from` and `when` as a preset reference overrides it;
`disabled` removes preset or inferred ids ([Effective rules](#effective-rules)).

**Export.** `project_export` returns the project as a preset file named `<slug>-<database>`: its references,
kinds and styles, saved queries as `[[pinned]]` (`pin` as `home`) and explicit presets as `requires`. The
prefixes, `disabled` and the automatic preset choice have no preset form and are left out.
`project_export_to(session_id, database, path)` writes that text to a path the user chose in the native
save panel (`choose_save_path(file_name, extension)` in `commands.rs`, from `tauri-plugin-dialog`'s Rust API;
the page has no dialog permission).

## The Project page

The Project page (`?project`, a `project` tab; frontend in `src/components/project/`) shows and edits the
rules of one database. It opens from the `project.open` command (palette `>Project`, no default key), the
status bar's `presets …` text and the address. `project_rules(session_id, database)`
(`rules/overview/`; the row types in `rows.rs`) returns everything it lists in one `ProjectRules`:

| Field | Means |
|---|---|
| `project` | the stored project |
| `prefix`, `suggestedPrefix` | the effective prefix and the suggestion from marker tables |
| `presets` | every loaded preset: `name`, `markerTables` (real names under the prefix), `applied` |
| `foreignKeys` | how many declared foreign keys apply |
| `references` | `ReferenceRow`s: project rules, then preset rules (last applied preset first, overridden ones left out, owned `[[meta]]` tables' `fk` references included), then foreign keys and inferred references, then inferred references the project switched off |
| `kinds`, `styles` | project rules (with their `index`), then the applied presets' rules |
| `tables` | `RuleTable { table, name }`: every real table and the name rules write it as (tables outside the prefixes are missing) |

A `ReferenceRow` has its `origin` (`project`, `preset`, `foreignKey`, `inferred` with `indexed`), the `id`
that `disable`, `enable` and `removeReference` take (`inferred:<table>.<column>` for inferred rows; none for
a foreign key, which is a fact and cannot be switched off), real `from` and `to`, `when` and `toWhen` as
SQL-like text (`meta_key = '_customer_user'`; `glob` reads `GLOB`), `label`, `disabled`, the
`ResolvedReference` that `reference_coverage` takes (none while it does not apply: switched off, or shadowed by
a foreign key), and `rule`: the stored rule of a project or preset row, or for an inferred row the project
rule that promotes it (`promote` adds it; an unconditional project rule replaces the inference).

Sections, each a section band with rows:
1. **Presets and prefix:** the prefix field (empty: the suggestion applies, shown as `suggested: wp_`), each
   preset with its marker tables and a switch (`presets: null` is auto; a switch stores the applied presets as
   an explicit list, "back to auto" clears it), and "Export as preset…".
2. **The rule form**, when one is open: a reference, kind or value-style form.
3. **References:** source, when, target, origin, coverage, actions. Project rows: check, edit, remove. Preset
   rows (`--o2`): check, disable. Foreign keys (`--o2`): check. Inferred rows: dashed target, `same name ·
   indexed` / `not indexed`, check, promote, disable. Switched-off rows are struck through with enable.
   Coverage runs on demand per row ("check") or for all rows one after another ("Check coverage for all"),
   and shows `resolved of total`, `· n are 0 or NULL`, up to three unresolved values, or "timed out".
4. **Value styles:** column or meta key, match, color (in its `--style-*` color), origin; project rows edit and
   remove, preset styles (shipped off) "switch on", which copies them into the project with `enabled = true`.
5. **Kinds:** column or meta key, kind, origin; project rows edit and remove. Preset kinds are read-only (a
   project kind rule comes first and so wins).

**From a value.** `value.addReference` (`r`, grid and object scopes as `value.` and `object.`),
`value.setKind` and `value.setStyle` (context menu "Add reference…", "Kind…", "Style…") call
`rule_draft(session_id, database, provenance)` (`rules/draft.rs`), which returns the value's column as rules
name it (`source`, e.g. `wc_orders_meta.meta_value`) and, on the value column of a `[[meta]]` table, `meta`:
the meta table's rule name, its key column and the key the row is stored under (read from the server). The
Project page opens with the form prefilled: a reference from the column with `when` `meta_key = '…'` on a meta
value; a kind or style scoped to the column, or to the meta key in that meta table (a style also takes the
value's text).

**Reference form.** Source and target are table and column pickers over `tables` and the schema cache (a
new target table preselects its single-column primary key); `when` is one `column = value` (a stored rule
with a richer `when` keeps it unchanged); label. While typing, `coverage_preview(session_id, database,
reference, replacing)` counts the unsaved rule (debounced): it checks the rule like `project_update`
(`addReference`, or `updateReference` of `replacing`) on a copy of the project, builds the effective rules
from that copy and counts exactly like a saved rule. Zero resolved values with orphans show "no source value
matches a target; the rule is probably wrong" with the unresolved sample. Saving calls `project_update`; an
error shows next to the field it names (`reference.from` at the source, `reference.when.*` at when,
`reference.to` and `reference.to_when.*` at the target, `reference.label` at the label). Kind and style
forms take the scope as rules write it (columns and globs without prefix, meta keys, meta tables).

Every change bumps `dataVersion`, so documents, grids and the page reload with the new rules.

## Effective rules

`rules/effective.rs` combines the applied presets and the project of a database into `EffectiveRules`, with
real table names and compiled conditions. It is cached per session and database and rebuilt when the schema
changes (DDL, refresh), a preset file changes or the project changes.

**Which presets apply** (`rules/applied.rs`, `Applied`):
- The **prefix** is the project's, else the suggestion: the shortest prefix under which all marker tables of
  some candidate preset exist (candidates: the project's `presets`, else every preset that loads), else none.
- With `presets` set, those presets apply, whatever tables exist. Otherwise every preset with
  `marker_tables` applies when they all exist under the prefix. A preset without marker tables applies only
  when listed or required.
- An applied preset brings the presets it `requires`, which apply before it.
- The **network prefix** is the project's, else the shortest prefix that holds all `network_tables` of the
  applied presets, else the prefix.
- A table name resolves under the network prefix when an applied preset lists it in `network_tables`, else
  under the prefix; when that table does not exist, the other prefix is tried. Rules on tables that exist
  under neither are dropped (presets describe optional tables too).

**Precedence.** For a column, the strongest reference wins:
1. a declared foreign key (a constraint is a fact; no rule overrides it, and a single-column primary key
   stays the row's identity);
2. project references;
3. preset references, the last applied preset first (a preset refines the presets it requires);
4. same-name inference.

**A table's own key.** Same-name inference never starts at a table's own single-column primary key. A
project or preset rule may: one-to-one extension tables share the key of the row they extend (a lookup
table whose primary key is the order id). Such a rule shows in `table_references` (both ends list it and
coverage counts it), while the column's kind stays the primary key: the row's identity is the stronger
fact, and the cell still opens its own row.

Within one source, conditional references come before unconditional ones. The column's kind is the first
unconditional reference; a conditional reference that comes before it decides per row (see below). Rules
of one source with the same `from` and `when` (one column pointing at two tables) become one reference with
several targets.

**Overrides and disabling.** A project reference with the same `from` and `when` as a preset reference
replaces it. `disabled` drops preset references by id (in every applied preset) and same-name inference by
`inferred:<table>.<column>` (real names, e.g. `inferred:wp_actionscheduler_logs.action_id`).

**Kinds.** Kind rules apply where no key or reference does (to columns the SQL type made text, number or
timestamp). The first matching rule wins, project rules before preset rules (`EffectiveRules::column_kind_rule`,
`meta_kind_rule`). `columns` globs match the
table's name without prefix and the column name (case-insensitively); `meta_keys` globs match the key of a
row of an applied `[[meta]]` table (owned or not), in every meta table unless `in` names some.

| Rule | Column kind | Cell |
|---|---|---|
| `money = { currency = [...] }` | `money` | the currency is the first listed column of the row with text, sourced from the same row; in documents also a column of the owner row, then a meta key of the document |
| `timestamp = "unix"` | `timestamp`, zone `unix` | `utc` from the leading digits (`<unix>:<id>` reads too); `0` is no time |
| `timestamp = "utc"` | `timestamp`, zone `utc` | `utc` from `YYYY-MM-DD HH:MM:SS` or `YYYY-MM-DD`; zero dates are no time |
| `timestamp = "local"` | `timestamp`, zone `local` | `utc` stays empty: the zone is not stored, so the value is never converted |
| `enum`, `number` | `enum`, `number` | |
| `secret` | `secret` | masked, even when it decodes as serialized data |

A temporal column without a rule is a timestamp without zone, shown raw.

**Per-row decisions** (`RulesLens::cells`), in memory over the page, without queries:
- a conditional reference whose `when` holds on the row (all `when` columns must be in the result);
- on a meta table's value column (with its key column in the result), the kind rule of the row's key;
- a value style whose columns or meta keys cover the cell and whose match holds.

They reach the page as `ResultSet.cell_kinds` (only where the kind differs from the column's) and
`ResultSet.styles`, sorted by row, then column; `valueAt` applies both.

**Targets.** A reference to a primary key addresses the row. Any other target addresses the table filtered
by the value, plus the `=` conditions of `to_when` (other `to_when` operators cannot be part of an address
and apply where documents query the target).

**Value styles** apply only when `enabled`; preset styles ship switched off, and a project switches one on
by adding it (with `enabled = true`) to its own `styles` (`project_update` with `addStyle`). The path: the
project's and the applied presets' enabled styles (project first) become `EffectiveRules.styles`;
`RulesLens::cells` matches each cell of a covered column (or of a meta value column whose row key matches
`meta_keys`, with the key column in the result) and adds `{ row, col, style }` to `ResultSet.styles` of browse
pages and query results; `valueAt` (`src/lib/values.ts`) puts it on the cell's `Value.style`. The first
matching style wins. The style is the scheme color name; no value is colored without a rule.

## Documents

`document` ([architecture.md](architecture.md#documents)) reads the rules of a row's table:

| Rule | In the document |
|---|---|
| `[[entity]]` | the row's `entity` (the first whose `filter` holds) and its `header` (`summary`); the entity shorthand `order 4711` |
| owned `[[meta]]` | one meta group per table, key/value rows in stored order, at most 50 |
| `[[reference]]` to the table | an incoming group per reference, named by `label`, with `columns` (meta keys of the source's `[[meta]]` table pivot in), at most 20 rows; left out when `to_when` does not hold on the row |
| `[[pattern]]` owned by the table | a pattern group per `name` and `table` |
| `[[kind]]` | every value's kind; money currencies from the row, the owner row or a meta key of the document; every timestamp goes into `events` |

Declared foreign keys and same-name inference add incoming groups without any rule, so every table with a
primary key has a document.

## `rules_info`

`rules_info(session_id, database)` returns `RulesInfo { prefix, networkPrefix, suggestedPrefix,
appliedPresets, availablePresets, entities, tableEntities }`. `entities` are `{ preset, entity }` with the
real table name; `tableEntities` maps a real table to its entity names. Entities with a `filter` come first
on their table: a row takes the first entity whose filter holds.


## Coverage

`reference_coverage(session_id, database, reference)` (`rules/coverage.rs`) counts how well one reference
resolves:

| Field | Means |
|---|---|
| `total` | source rows the reference applies to (its `when` holds) |
| `resolved` | of those, rows with a target row |
| `zeroOrNull` | rows whose value is NULL or `0`: "no reference", not orphans (a composite reference: any NULL part) |
| `sampleUnresolved` | up to ten distinct values without a target (composite values joined by `,`) |
| `timedOut` | the count hit the query timeout (counts are 0), or only the sample did (counts stand, sample empty) |

Orphans are `total - resolved - zeroOrNull`.

**Naming a reference.** The argument is a `ResolvedReference` exactly as `table_references` returned it
(`from`, `to`, `by`). That covers every source with one shape: declared foreign keys (by constraint), project
and preset rules (by rule id; a column with two targets has one reference per target), and inferred
references (one per target table). The reference must be among the current `references_from` of its
source table, else the command fails; so every table and column in the SQL comes from the schema, and a
disabled or replaced rule has no coverage.

**SQL.** Read-only `SELECT`s on the metadata pool, tables qualified with the database, each bounded by the
session query timeout (on timeout the server query is killed, as for other pooled queries):
- one pass counts the total, `zeroOrNull` and the rows for which `EXISTS` finds a target row (not a
  `LEFT JOIN` count, so a non-key target that matches several rows counts its source row once);
- only when orphans exist, a second query reads the sample.

The join compares with the columns' collation, like following the reference does, so the counts match what
a click finds. Conditions are said in SQL with the same meaning as in memory:
- `when` filters the source rows, `to_when` joins on the target row;
- `=`, IN, `glob` (as LIKE) and `like` test the plain column (an index still narrows) and the column cast
  to binary (exact, whatever the collation);
- `regexp` runs as `REGEXP '(?-i)<pattern>'` (MySQL rejects a binary operand), in the server's regular
  expression dialect (ICU on MySQL, PCRE on MariaDB), which agrees with the in-memory one on common patterns;
- `not` wraps the test in `NOT (...)`, so NULL matches neither the test nor its negation.

A reference from a column to the same column of its table (shared emails, external ids) counts a row as
resolved only when another row matches (by primary key); a table without one finds the row itself.
