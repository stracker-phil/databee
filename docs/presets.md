# Presets

A preset is a TOML file of rules that tells DataBee what the tables of an application mean: which columns
reference which rows, what kind of value a column or meta key holds, which values get a color, and which
queries, anomalies and actions belong to it. The code is generic (`src-tauri/src/rules/`); application
knowledge lives only in preset files. Rules follow [`.claude/rules/presets.md`](../.claude/rules/presets.md).

Status: presets are parsed, checked and listed (`presets_list`); projects are stored and edited
(`project_get`, `project_update`, `project_export`). The rules lens that applies presets and projects to
browsing comes next; until then no rule changes what the app shows.

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

**Conditions** (`when`, `to_when`, `owner_when`, an action's `when`) map a column to a match; all must hold:

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

**Overrides.** A project reference with the same `from` and `when` (the same default id) as a preset
reference overrides it; `disabled` removes preset or inferred ids. Applying both comes with the effective
rules (next part of this step).

**Export.** `project_export` returns the project as a preset file named `<slug>-<database>`: its references,
kinds and styles, saved queries as `[[pinned]]` (`pin` as `home`) and explicit presets as `requires`. The
prefixes, `disabled` and the automatic preset choice have no preset form and are left out.
