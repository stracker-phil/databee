# Presets

A preset is a TOML file of rules that tells DataBee what the tables of an application mean: which columns
reference which rows, what kind of value a column or meta key holds, which values get a color, and which
queries, anomalies and actions belong to it. The code is generic (`src-tauri/src/rules/`); application
knowledge lives only in preset files. Rules follow [`.claude/rules/presets.md`](../.claude/rules/presets.md).

Status: presets are parsed, checked and listed (`presets_list`); projects are stored and edited
(`project_get`, `project_update`, `project_export`); references, kinds and value styles apply to browse
pages, query results and `table_references` ([Effective rules](#effective-rules)), and `rules_info` reports
the prefixes, presets and entities. Entities, patterns, pinned queries, anomalies, actions and find sources
are parsed but not used yet.

## Where presets live

| Source | Location | Notes |
|---|---|---|
| Built-in | `src-tauri/presets/*.toml`, compiled in (`rules::catalog::BUILT_IN`) | none yet |
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
| `{id}` | the row's primary key, as a literal | patterns, row-scope actions |
| `{owner.<column>}`, `{meta.<key>}` | the owner row's value, as a literal | patterns |
| `{param.<name>}` | a declared param, as a literal (NULL when empty) | actions |

Braces with any other content are SQL and stay as written (`'^[0-9a-f]{32}$'`).

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

Within one source, conditional references come before unconditional ones. The column's kind is the first
unconditional reference; a conditional reference that comes before it decides per row (see below). Rules
of one source with the same `from` and `when` (one column pointing at two tables) become one reference with
several targets.

**Overrides and disabling.** A project reference with the same `from` and `when` as a preset reference
replaces it. `disabled` drops preset references by id (in every applied preset) and same-name inference by
`inferred:<table>.<column>` (real names, e.g. `inferred:wp_actionscheduler_logs.action_id`).

**Kinds.** Kind rules apply where no key or reference does (to columns the SQL type made text, number or
timestamp). The first matching rule wins, project rules before preset rules. `columns` globs match the
table's name without prefix and the column name (case-insensitively); `meta_keys` globs match the key of a
row of an applied `[[meta]]` table (owned or not), in every meta table unless `in` names some.

| Rule | Column kind | Cell |
|---|---|---|
| `money = { currency = [...] }` | `money` | the currency is the first listed column of the row with text, sourced from the same row (owner columns and sibling meta keys come with documents) |
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
by adding it (with `enabled = true`) to its own `styles`.

## `rules_info`

`rules_info(session_id, database)` returns `RulesInfo { prefix, networkPrefix, suggestedPrefix,
appliedPresets, availablePresets, entities, tableEntities }`. `entities` are `{ preset, entity }` with the
real table name; `tableEntities` maps a real table to its entity names. Entities with a `filter` come first
on their table: a row takes the first entity whose filter holds.

