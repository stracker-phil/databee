# Presets

A preset is a TOML file of rules that tells DataBee what the tables of an application mean: which columns
reference which rows, what kind of value a column or meta key holds, which values get a color, which icon a
table shows, and which queries, anomalies and actions belong to it. The code is generic (`src-tauri/src/rules/`); application
knowledge lives only in preset files. Rules follow [`.claude/rules/presets.md`](../.claude/rules/presets.md).

Status: presets are parsed, checked and listed (`presets_list`); projects are stored and edited
(`project_get`, `project_update`, `project_export`, `project_export_to`) and shown on the
[Project page](#the-project-page) (`project_rules`, `rule_draft`, `coverage_preview`); references, kinds and value styles apply to browse
pages, query results and `table_references` ([Effective rules](#effective-rules)), `rules_info` reports
the prefixes, presets and entities, and `reference_coverage` counts how many rows of a reference resolve
([Coverage](#coverage)). `document` builds a row's document from meta tables, references, entities,
patterns and kinds ([Documents](#documents)). Pinned queries, anomalies, entities with a `created` column and
the recent objects of a project make up the [readout](#the-readout); actions plan on rows and tables; find
sources feed the palette.

## Where presets live

| Source | Location | Notes |
|---|---|---|
| Built-in | `src-tauri/presets/*.toml`, compiled in (`rules::catalog::BUILT_IN`) | `wordpress`, `woocommerce`, `action-scheduler` ([Built-in presets](#built-in-presets)) |
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

`summary` is what a row shows wherever it is named: the document header, peeks, palette hits and the
readout. Each name in order is a column of the table (any case) or a meta key of the row's `[[meta]]` tables
(the first stored row of the key); a name that is neither, or a meta key the row lacks, shows as NULL, so an
empty configured field is visible and never silently missing. Money shows with its currency (from the row,
then its meta keys). A row whose entity has no `summary`, or a table without an entity, shows its first 4
columns outside the primary key that are neither NULL nor `''`. The entity is the one the row's document
shows (the first whose `filter` holds). Names are not validated when the preset loads: a meta key only proves
itself in the data.

### `[[kind]]`

Exactly one of `money`, `timestamp`, `enum = true`, `secret = true`, `number = true`, scoped to `columns`
(globs on both sides: `*.*_gmt`; the table part compares case-sensitively, the column part
case-insensitively) and/or `meta_keys` (globs). `in` narrows `meta_keys` to some `[[meta]]`
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
`enabled = false` and a project switches them on by id.

```toml
[[style]]
id = "failed_orders"                # optional
columns = ["orders.status"]
match = "="
value = "failed"
color = "red"
enabled = false
```

**Ids.** Without `id`, a style's id is `style:<columns>|<meta_keys>|<in><match>`: each list joined with `,`,
trailing empty parts left out, the match written as in reference ids (`=x`, `=like(x)`, `=regexp(x)`). So
`style:orders.status=failed`, `style:|_status|order_meta=like(ok%)`. When two styles of a preset would get the
same id, both get `-><color>` appended. Ids are unique per preset; a project switches a style and overrides it
by its id (`StyleRule::default_id`, `fill_style_ids` in `rules/preset.rs`).

### `[[icon]]` table icons

```toml
[[icon]]
table = "users"                     # a name without prefix
icon = "user"

[[icon]]
table = "*meta"                     # or a glob over the names without prefix
icon = "menu"
```

`icon` is one of a curated set of lucide names (`rules::icons::TableIcon`; the page maps each to its component,
`src/components/TableIcon.tsx`; `table` draws lucide's `table-2`):

```
table database server list list-checks layers box archive hash key key-round lock log-in shield shield-check
user user-round users user-cog id-card file-text files file file-code file-braces file-clock newspaper
book-open graduation-cap image film music paperclip folder folder-tree menu tag tags bookmark star heart
flag message-square message-circle mail inbox send bell megaphone phone shopping-cart shopping-bag store
receipt package barcode truck warehouse credit-card wallet banknote coins landmark badge-percent gift ticket
building briefcase globe map-pin map route languages clock calendar calendar-clock timer settings
sliders-horizontal wrench activity chart-bar code terminal bug webhook plug puzzle blocks git-branch
workflow link
```

Another name (`none` included: every table has an icon) is a load error naming the field (`[[icon]] #2 (line
30)`, field `icon`: unknown variant …); a `table` is unique per preset.

**Which icon a table gets** (`icons::table_icons`, `EffectiveRules.icons`): for each real table, its names
without prefix (`Applied::rule_names`) are matched first against the project's icon rules, then against each
applied preset's, the last applied first; within one source a rule naming the table exactly beats a glob, and
among globs the first in file order wins. The first source with a match decides; without one the table gets
`table` (`TableIcon::default`), as does every table outside the prefixes. Globs compare case-sensitively. The
icon is shown before the table's name, before every reference into the table and before every key of its rows
([ux.md](ux.md#value-grammar)); it is chrome, the table name stays.

### `[[pattern]]` groups matched by SQL

```toml
[[pattern]]
name = "log"                        # same name and table on one owner: one group (OR)
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

A pinned query is a read-only saved query of its preset. `saved_queries(session_id, database)`
(`saved/mod.rs`, `list`) returns the project's saved queries, then the global ones, then the pinned queries of
every applied preset in preset order, each as `SavedEntry { id, title, sql, pin, origin, readOnly }`:
`origin` is a `SavedOrigin` (`project`, `global` or `preset { preset }`, shown as `preset:<name>`), `title` defaults to the `id`, and `{p}`/`{np}` in the
title and SQL are replaced with the project's prefixes (`Applied::expand`), so the SQL runs as
shown. The palette's `q:` lists them; a pinned query opens as an unsaved query tab titled `copy of <title>`.
`copy_preset_query_to_project(session_id, database, preset, id)` stores it, prefixes replaced, as a project
query with a fresh id, which can then be changed and pinned like any saved query (`update_saved_query`;
`save_query` adds one, `remove_saved_query` removes a project or global one). `home` is shown on the
[readout](#the-readout). Favorites of older versions were moved into saved queries ([backend.md](backend.md#persistence)).

### `[[anomaly]]`

Shown on the [readout](#the-readout) while `sql` returns a non-zero number; NULL, no row or text that is no
number hides it.

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
dropped (`EffectiveRules.actions`). Each listed `ObjectAction` carries `undo` and `writes`
(`document/actions/mod.rs`), one `ActionWrite` per step in planning order: `set` with the stored table name
(prefix included) and the `set` columns (sorted by name), `deleteRows` or `freeSql` with the action's table.
An update step on a table the database lacks is left out, as planning skips it. Free SQL may touch other
tables; only its own table is named.

**Planning** (`planned` in `document/actions/plan.rs`, for `preview_action` and `plan_action`):
- Params are checked first: unknown names and missing required values (without `default`, not `optional`)
  are refused, an empty value counts as missing (`optional` makes it NULL), `type = "int"` must parse as an
  integer, and unless `free` the value must be one of the `choices_sql` rows.
- A row action is refused when its `when` no longer holds on the row.
- `update`: each step selects its rows by the row itself (the first step without `table` and `key`), or by
  `key` (default: the step table's primary key) `= {id}`, plus the action's `when` on the action's table and
  the step's `where`; a step on a missing table is skipped. A table action selects by `when` and `where`.
  The steps become one `where_changes::plan_update` plan ([database.md](database.md#editing-rows)). A text `set` value
  that is a single placeholder is that placeholder's literal (NULL stays NULL); other text is one string
  literal with the placeholders filled in as text. `{ sql = "…" }` is an expression.
- Every `set` value of every step is evaluated once, at planning, in one `SELECT <value>, <value>, …`
  (`resolve`), and the statements write the resulting literals. `UTC_TIMESTAMP()` is therefore the same
  moment in every step, and the preview shows exactly what is written. A `{ sql = "…" }` value cannot read
  the row it is written to (there is no `FROM`), and a value that resolves to binary data is refused.
- `delete`: the same selection, planned by `where_changes::plan_delete`.
- `sql`: run as written, without undo; the first value of `preview_sql` goes into the description.

**Preview** (`preview_action`, `document/actions/preview.rs`): the same planning without queueing. An
`ActionPreview` holds the description, `canUndo`, and per written row (`PreviewRow`) the stored table, the
`RowKey` and either `update` with `ColumnWrite { column, old, new }` per set column (old from the
before-image, new as resolved; raw stored values) or `delete`. Free SQL has no rows; its `sql` holds the
statements, whose rows are not known in advance. The session keeps the latest previewed plan (`Previews`);
`queue_action_preview(id)` moves exactly that plan to the pending changes, so a resolved `UTC_TIMESTAMP()`
does not move between preview and queue. An older preview id is refused.

### `[[find]]`

Extra sources for find everywhere (the palette's `=value`), besides meta values and entity search columns
([architecture](architecture.md#palette-search)).

```toml
[[find]]
table = "log"
columns = ["message"]
match = "contains"                  # default "exact"
```

`columns` is required and names at least one column (`FindSource`'s `Check`): an empty list is a load error
on field `columns`.

## Placeholders

Preset SQL is trusted configuration in the MySQL/MariaDB dialect. Placeholders (`rules::placeholder`):

| Placeholder | Renders | Allowed in |
|---|---|---|
| `{p}`, `{np}` | the prefix, the network prefix, as text | all SQL and titles |
| `{id}` | the row's primary key, as a literal (a number when it parses as one) | patterns, row-scope actions (not `choices_sql`) |
| `{owner.<column>}`, `{meta.<key>}` | the owner row's value, as a literal | patterns |
| `{param.<name>}` | a declared param, as a literal (NULL when empty) | actions |

Braces with any other content are SQL and stay as written (`'^[0-9a-f]{32}$'`).

The metadata pool has no default database, so in SQL that documents and actions run (pattern `where`,
entity `filter`, action SQL, `choices_sql`, `preview_sql`) every `{p}` and `{np}` outside quotes is qualified
with the database (`placeholder::qualify_prefixes`): `{p}options` reads and writes
`` `db`.wp_options `` whatever the connection's default database is. Inside quotes (`'{p}capabilities'`)
they stay plain text.

Prefixes are filled in one way: `Applied::expand` replaces `{p}` and `{np}` in any preset text (titles,
saved and pinned queries, anomaly text and `where`), and `Ctx::preset_sql` qualifies SQL a document runs, then
expands it. The value placeholders are filled in afterwards by `placeholder::render`, which names the first
placeholder without a value (patterns skip the rule, actions refuse with "`…` has no value here").

## Errors

Each rule is read on its own, so an error names the file, the rule and the field:

```
/…/presets/shop.toml: [[reference]] #3 (line 40): `when.meta_key`: unknown condition `glib`, expected glob, like, regexp or not
/…/presets/shop.toml: [[action]] #1 (line 88): `update[0].set.status`: `{param.state}` names no declared param
```

TOML syntax errors keep the parser's line and column.

## Recipes

### Write a preset

1. Create `<config dir>/presets/<name>.toml` (`~/Library/Application Support/com.philippstracker.databee/presets/`).
   The catalog rereads the folder when a file is added, removed or saved; no restart.
2. Start with the header. `name` is required; the preset applies on its own only when all `marker_tables`
   exist under the prefix, or when a project lists it or another preset `requires` it.

   ```toml
   [preset]
   name = "acme-forms"
   requires = ["wordpress"]
   marker_tables = ["acmeforms_entries"]
   ```
3. Add rules section by section ([Format](#format)), table names without the prefix.
4. Check it: the Project page lists the preset with its marker tables and a switch; a file with an error is
   listed with the message ([Errors](#errors)) and applies nowhere. Open a row of one of its tables to see
   the document the rules build.

### Add references for a plugin

A plugin's tables usually point at WordPress rows by id. Same-name inference only links a column named exactly
like another table's primary key (`action_id`), so declare the rest. Example for a form plugin with an entries table and its meta:

```toml
[preset]
name = "acme-forms"
requires = ["wordpress"]                # applies after it, so its tables and entities are known
marker_tables = ["acmeforms_entries"]

[[meta]]                                # one meta group per entry; declares entrymeta.entry_id → entries
table = "acmeforms_entrymeta"
owner = "acmeforms_entries"
fk = "entry_id"
key = "meta_key"
value = "meta_value"

[[reference]]
from = "acmeforms_entries.user_id"      # 0 or NULL: no reference
to = "users.ID"
label = "form entries"                  # the incoming group on the user's document
columns = ["id", "form_id", "status"]

[[reference]]
from = "acmeforms_entries.post_id"
to = "posts.ID"

[[reference]]
from = "usermeta.meta_value"            # a key of a WordPress meta table pointing into the plugin
when = { meta_key = "_acme_last_entry" }
to = "acmeforms_entries.id"

[[entity]]
name = "form_entry"
table = "acmeforms_entries"
summary = ["form_id", "status"]
created = "created_at"
```

Then open the Project page: it counts the coverage of every reference on its own, and a reference that
resolves none of its values is probably wrong. For one database only, add the same references on the Project page (or with
"Add reference…" on a value) instead: they are stored in the project and can be exported as a preset later.

### Add a value style

For one database: "Style…" on a value (context menu) opens the Project page with a style form for that
column, or that meta key in its meta table, prefilled with the value. A preset style is switched on or off
with "enable" / "disable" in the Value styles section, which stores only its id; "edit" stores a project
override under the same id.

In a preset, a style is scoped to columns or meta keys, never global, and `enabled` defaults to `true` (the
built-in presets set `enabled = false` so a project opts in):

```toml
[[style]]
meta_keys = ["_acme_status"]
in = ["acmeforms_entrymeta"]
match = "REGEXP"                        # "=", "LIKE" or "REGEXP"
value = "^(spam|failed)$"
color = "red"                           # red, green, yellow, blue or purple
```

The first matching style wins, project styles before preset styles (an override takes its preset style's
place).

### Give a table an icon

For one database: on the Project page (All rules) or in the table's Structure view, "icon…" opens the picker;
a choice stores a project rule for the table (`setIcon`), "Reset to preset" removes the project rule. A table
no rule names shows `table (default)`. In a preset, name the table or a glob:

```toml
[[icon]]
table = "acmeforms_entries"
icon = "list-checks"
```

The WordPress preset's `*meta` already gives every table ending in `meta` (a plugin's too) the `menu` icon; a
rule naming the table exactly, or any rule of a later preset or the project, beats it.

### Remove the built-in presets

- **For one database:** on the Project page, switch the presets off. The switches store an explicit
  `presets` list; "back to auto" returns to marker-table detection.
- **On one machine:** a user preset with the same `name` replaces the built-in. An empty one switches it
  off everywhere:

  ```toml
  [preset]
  name = "woocommerce"
  ```

  Without `marker_tables` it applies only where a project lists it or a preset requires it, and then
  adds nothing.
- **From the app:** delete the files and their `BUILT_IN` lines ([adapting.md](adapting.md#removing-the-wordpress-presets)).

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
| `icons` | `[[icon]]` rules, one per `table`; they beat every preset icon for the tables they name. A stored rule that no longer parses (an icon the set dropped, such as the former `none`) is left out with a logged warning (`known_icons`), so the presets' icon or `table` applies again |
| `disabled` | ids of preset or inferred rules switched off, preset styles that ship on included |
| `enabled` | ids of preset styles that ship off (`enabled = false`) and are switched on |
| `saved` | saved queries `{ id, title, sql, pin }`; `pin` has the shape of `[[pinned]] home`. Saved queries for every project are `global` in `projects.json`, beside `projects` |
| `accent` | the accent while this database is open, one of the settings' accents (`ACCENTS`); left out when unset, so the app's accent applies. An unknown or retired accent loads as unset (`known_accent`). Personalization, not a rule ([Accent](#projects)) |
| `watched` | values the readout shows (`WatchedValue { id, label, table, column, row, path }`), in the order they were added; left out when empty. Personal, not a rule ([Watched values](#watched-values)) |

**Editing.** `project_update(session_id, database, change)` applies one `ProjectChange` (`op`):
`addReference`, `updateReference` and `removeReference` by id; `addKind`, `updateKind`, `removeKind` by
index (kinds have no id); `addStyle`, `updateStyle`, `removeStyle` by id; `enableStyle` and `disableStyle` a
preset or project style's id (idempotent: a preset style's id is added to or removed from `enabled` or
`disabled`, depending on how the style ships; a project style's `enabled` is set; `switch_style`); `disable` and `enable` a reference or inferred id; `setIcon` stores the project's icon rule for its `table`
(replacing one with the same `table`; a name must resolve like any rule table, a glob is not checked) and
`removeIcon` removes it by `table`, which brings back the presets' icon; `setPrefix`, `setNetworkPrefix`,
`setPresets`. The accent is not a `ProjectChange` ([Accent](#projects)). Before storing, every table a rule names must exist under the
prefix or the network prefix (the stored ones, else the suggestions), resolved as the effective rules resolve
it (`Applied::existing_table`, see below), and every literal column (glob parts are
not checked), `when` and `to_when` column must exist in its table. A prefix must start some table name,
preset names must load. The error names the field: ``` `reference.when.meta_key`: `wp_posts` has no column
`meta_key` ```. A reference without `id` gets its default id; when the project already uses it, `-><to>` is
appended. A style without `id` likewise gets its default id or `-><color>`; an exact copy of a project style
(same scope, match and color) is refused. `updateStyle` of a preset style's id stores a project override.

**Overrides.** A project reference with the id of a preset reference, or the same `from` and `when`,
overrides it; `disabled` switches off project, preset or inferred ids ([Effective rules](#effective-rules)). A project style with the id
of a preset style overrides that style; it stays on or off with the preset style's switch, and removing it
brings back the preset's rule. Adding a style whose id is a preset style's (`addStyle`) also switches it on.

**Migration.** Before styles had ids, a preset style was switched on by copying it into the project's
`styles`. At startup (`AppState::open`, `ProjectsStore::migrate_styles`) every project style stored without
an id is migrated against the loaded presets: an enabled copy of a preset style (same scope, match and color)
becomes that style's id in `enabled` (or leaves `disabled` for one that ships on), an exact duplicate of a
project style is dropped, and any other style gets its default id (or `-><color>`); one under a preset
style's id overrides it and switches it on. Projects without such styles are not rewritten.

**Export.** `project_export` returns the project as a preset file named `<slug>-<database>`: its references,
kinds, styles and icons, saved queries as `[[pinned]]` (`pin` as `home`) and explicit presets as `requires`. The
prefixes, `disabled`, the automatic preset choice, the accent and the watched values have no preset form and are
left out.
`project_export_to(session_id, database, path)` writes that text to a path the user chose in the native
save panel (`choose_save_path(file_name, extension)` in `commands.rs`, from `tauri-plugin-dialog`'s Rust API;
the page has no dialog permission).

**Accent.** A database can have its own accent, so the open database shows at a glance. The hues are the
settings' accents; status colors stay separate. `project_set_accent(session_id, database, accent)` (`ProjectsStore::set_accent`)
stores it for the session's slug and the database, `null` clears it, and an accent outside `ACCENTS` is refused
with the field `accent`. It is a command of its own, not a `ProjectChange`: it changes no rule, so it needs no
schema check, does not bump `dataVersion` (nothing reloads) and is no line in [Your changes](#your-changes).
`project_accents()` (`ProjectsStore::accents`) returns every stored accent by `<slug>/<database>` from the
projects file alone (no server query); the page loads it at startup and after a slug rename, which moves the
accent with its project. Frontend: [frontend.md](frontend.md#styling).

## The Project page

The Project page (`?project`, a `project` tab; frontend in `src/components/project/`) shows what a project
changes about the presets of one database and what looks broken, and behind "All rules…" edits every rule,
grouped by table. It opens from the `project.open` command (palette `>Project`, no
default key), the status bar's `presets …` text and the address. `project_rules(session_id, database)`
(`rules/overview/`; the row types in `rows.rs`) returns everything it lists in one `ProjectRules`:

| Field | Means |
|---|---|
| `project` | the stored project |
| `prefix`, `suggestedPrefix` | the effective prefix and the suggestion from marker tables |
| `presets` | every loaded preset: `name`, `markerTables` (real names under the prefix), `applied` |
| `foreignKeys` | how many declared foreign keys apply |
| `tables` | `RuleTable { table, name }`: every real table and the name rules write it as (tables outside the prefixes are missing) |
| `changes` | `ProjectChangeLine`s: every deviation from the presets as they ship ([Your changes](#your-changes)) |
| `groups` | `TableRules` per real table with any rule (an icon counts), sorted by name, then one with `table: null` for value styles and kinds whose scope covers no table here |

A `TableRules` holds `references` (the `ReferenceRow`s from the table, by source column), `referencedBy`
(`{ from, to, when, disabled }` of references from other tables to it), `styles` and `kinds` (each rule in
every table one of whose columns its column globs cover, or every `[[meta]]` table its meta keys and `in`
cover), `entities` (`{ preset, name, filter }`) and `actions` (`{ preset, name, scope }`) on the table, and
`icon`: the `IconRow` of the rule that applies (`origin` project or preset, `icon`, `pattern` as the rule writes
the table, and `shipped`, the presets' rule a project rule overrides); `null` when no rule names the table,
which then shows `table`.

A `ReferenceRow` has its `origin` (`project`, `preset`, `foreignKey`, `inferred` with `indexed`), the `id`
that `disable` and `enable` take, real `from` and `to`, `when` and `toWhen` as SQL-like text
(`meta_key = '_customer_user'`; `glob` reads `GLOB`), `label`, `disabled`, `edited`, the `ResolvedReference`
that `reference_coverage` takes (none while it does not apply: switched off, or shadowed by a foreign key),
and `rule`: the stored rule, or for an inferred row the project rule that would replace it (empty id).

**One model for every reference.** Project, preset and inferred references each have a switch and "edit";
a foreign key is a fact (no id, no switch). Editing stores a project rule:
- a project row is updated by its id (`updateReference`);
- a preset row becomes a project rule under the preset rule's id (`addReference` with that `id`), which
  overrides it (as does a project rule with the same `from` and `when`); its row keeps the preset origin with
  `edited`, and `id` is the project rule's;
- an inferred row becomes an unconditional project rule from its column, which replaces the inference; its
  row keeps the inferred origin with `edited`.

"reset" removes the project rule (`removeReference`), which brings back the preset rule or the inference;
project rows have "remove". The switch is `disable` / `enable` by `id`: a project rule's id switches the
project rule (and with it the preset rule it overrides); a switched-off unconditional project rule keeps the
inference on its column off.

Layout:
1. **Header**, one row: a switch per preset (marker tables in its tooltip; `presets: null` is auto; a switch
   stores the applied presets as an explicit list, "back to auto" clears it), the prefix field (empty: the
   suggestion applies, shown as `suggested: wp_`), the accent (a swatch per accent and "default (app accent:
   orange)", [Accent](#projects)), "Export as preset…", and a segmented control
   **Overview | All rules** (`ProjectTab.section`, default the overview).
2. **Overview:** "Your changes", "Problems", then "All rules…".
3. **All rules:** a filter that narrows the tables by name, "Refresh coverage", then **a band per table** (collapsible, remembered per database like a document group): references (source
   column and `when`, target and `where`, origin, coverage, switch, actions), a "referenced by" line, value
   styles, kinds, then read-only entity and action lines. Origins read `project`, `preset woocommerce`,
   `inferred · same name · indexed`, `foreign key <constraint>`, with `· edited in project` for an edited row.
   Value styles follow the reference pattern: a switch (`enableStyle` / `disableStyle`, project styles too),
   edit (a preset style stores an override under its id), reset (`removeStyle` of the override) or, for a
   project style, remove. Preset kinds are read-only (a project kind rule comes first and so wins); project
   kinds have edit and remove. Each table band starts with its icon line: the icon, its origin and pattern
   (`preset wordpress: *meta`, `project: users · edited in project, was user`) and "icon…", the picker
   (a filter field over every icon, "Reset to preset" or "Remove <pattern>" for a project rule); without a rule
   the line reads `table (default)`. The
   Structure view shows the same control.

**Coverage** runs on its own: when the page opens, every reference that applies is counted with
`reference_coverage`, three at a time, and leaving the page stops the queue. Results are cached per database
for the session (`coverage` in the project slice); "Refresh coverage" counts again. A row shows `resolved of
total` with a bar (resolved, then 0 or NULL); `n are 0 or NULL` and up to three unresolved values are in
its tooltip, or it reads "timed out".

**From a value.** `value.addReference` (`r`, grid and object scopes as `value.` and `object.`),
`value.setKind` and `value.setStyle` (context menu "Add reference…", "Kind…", "Style…") call
`rule_draft(session_id, database, provenance)` (`rules/draft.rs`), which returns the value's column as rules
name it (`source`, e.g. `wc_orders_meta.meta_value`) and, on the value column of a `[[meta]]` table, `meta`:
the meta table's rule name, its key column and the key the row is stored under (read from the server). The
Project page opens with the form prefilled: a reference from the column with `when` `meta_key = '…'` on a meta
value; a kind or style scoped to the column, or to the meta key in that meta table (a style also takes the
value's text).

**The rule sheet.** Every form (from a row's "edit", or from a value) opens in a modal sheet styled like
the palette over the Project page: ⎋ closes it, ⌘↵ saves.

**Reference form.** Source and target are table and column pickers over `tables` and the schema cache (a
new target table preselects its single-column primary key); `when` is one `column = value` (a stored rule
with a richer `when` keeps it unchanged); label. While typing, `coverage_preview(session_id, database,
reference, replacing)` counts the unsaved rule (debounced): it checks the rule like `project_update`
(`addReference`, or `updateReference` of `replacing`) on a copy of the project, switches it on there, builds
the effective rules from that copy and counts exactly like a saved rule. Zero resolved values with orphans show "no source value
matches a target; the rule is probably wrong" with the unresolved sample. Saving calls `project_update`; an
error shows next to the field it names (`reference.from` at the source, `reference.when.*` at when,
`reference.to` and `reference.to_when.*` at the target, `reference.label` at the label). Kind and style
forms take the scope as rules write it (columns and globs without prefix, meta keys, meta tables).

Every change bumps `dataVersion`, so documents, grids and the page reload with the new rules.

### Your changes

`ProjectRules.changes` (`rules/overview/changes.rs`) lists only what the project changes, one
`ProjectChangeLine` each, in this order (`Overview::changes`): settings, references, value styles, kinds,
icons, then switched-off ids that name nothing here.

| Field | Means |
|---|---|
| `action`, `rule` | `added`, `edited`, `switchedOff`, `switchedOn` or `set`; `reference`, `valueStyle`, `kind`, `icon`, `prefix`, `networkPrefix`, `presets` or `unknown` |
| `origin`, `id` | the preset or inferred rule the change is about (none for added rules and settings), and the rule's id |
| `text` | the rule or setting as it applies now, with real names: `wp_wc_orders.billing_email when … → wp_users.user_email`, `wc_orders.status = 'wc-failed' · red`, `wc_orders.total · money`, `wp_` |
| `was` | an edited rule's shipped form; the suggested prefix; the presets auto would apply |
| `off` | an added rule stored switched off |
| `table` | the real table the rule is about (a style or kind: the first its scope covers); the line opens its Structure view |
| `undo` | the `ProjectChange` that undoes it: `removeReference` / `removeStyle` / `removeKind` / `removeIcon` (added or edited), `enable` / `enableStyle` (switched off), `disableStyle` (a preset style switched on), `setPresets` / `setPrefix` / `setNetworkPrefix` with `null` |
| `edit` | the project rule the line opens in its form (`reference` / `style` by id, `kind` by index) |

The deviations: an explicit preset list, a prefix or network prefix; a project reference (added; or edited,
when it overrides a preset rule or replaces an inference); a switched-off preset or inferred reference; a
project style (added, or edited when its id is a preset style's); a preset style switched on or off; a
project kind; a project icon (edited when a preset gives the first table it covers an icon, with `was` the
preset's `pattern · icon`; `text` is the real table, or the glob, and the icon; no `edit`). An edited rule that
is also switched off gives two lines. The page renders each line as a
sentence ("switched off reference · preset woocommerce · guest_orders  wp_wc_orders.billing_email → …
[reset]") with "edit" where the rule is listed and the undo labelled "remove" (added), "switch off"
(switched on) or "reset"; without lines it says "No changes — this database uses the presets as shipped."

### Problems

The overview counts coverage like the per-table listing (same cache) but lists only problems: a reference
with values that point nowhere ("5 of 40 point nowhere (98, 99)"), one whose count failed, and each
`[[anomaly]]` whose query failed (from the readout's `anomalies`). A share of 0 or NULL is never a problem by
itself: WordPress stores 0 for "none" (`comment_parent`, a term's `parent`); the per-table listing and the
Structure view still show it. "Open rows" opens the source table filtered by `Coverage.rows.unresolved`, "Open
table" the table. Without problems it says "No problems found."

## The Structure view

A table tab's Structure view (frontend in `src/components/structure/`) shows one row per column with the
rules that apply to it. `table_rules(session_id, database, table)` (`rules/profile.rs`) builds a
`TableProfile` from the Project page's rows (`overview::project_rules`) and the table's structure, so its
switches, edits and resets are the Project page's:

| Field | Means |
|---|---|
| `table`, `ruleName` | the real table and the name rules write it as (`null` outside the prefixes) |
| `icon` | the table's `IconRow`, as on the Project page |
| `columns` | a `ColumnProfile` per column in table order: `column` (`ColumnInfo`), `primaryKey`, `indexes` (`{ name, unique, position }` of each index that holds it), `kinds`, `references`, `styles` |
| `meta` | `MetaKeyRules` of the table when it is a `[[meta]]` table, then of each meta table that describes its rows: `table`, `keyColumn`, `valueColumn`, the `kinds` and `styles` whose `meta_keys` (and `in`) cover it, and the conditional `references` from its value column |
| `referencedBy` | the `ReferenceRow`s from other tables to it, with origin and controls |
| `entities` | `{ preset, name, filter, summary, created }` of the table's entities |
| `actions` | `{ preset, name, scope, writes }`; `writes` is the same `ActionWrite` list documents carry (`rules::actions::writes`) |
| `ddl` | `SHOW CREATE TABLE`, or `null` |

- **Kinds** per column are the `KindRow`s whose column globs cover it, in precedence order; `applies` marks the
  first (the one values get), `pattern` names the glob that matched when it is one (`*.*_gmt`).
- **Value styles** per column are the `StyleRow`s whose column globs cover it, with the same `pattern`.
- **References** per column are the `ReferenceRow`s from it (a composite key lists under its first column),
  switched-off ones included; on a meta table, the conditional references from its value column list in its
  meta-keys block instead.
- Coverage counts as on the Project page (`reference_coverage`, cached per database).

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

**Overrides and disabling.** A project reference with the id of a preset reference, or the same `from` and
`when`, replaces it. `disabled` drops project references and preset references by id (in every applied
preset) and same-name inference by `inferred:<table>.<column>` (real names, e.g.
`inferred:wp_actionscheduler_logs.action_id`). A switched-off unconditional project reference keeps the
inference on its column off, so switching off an edited inferred reference switches the reference off.

**Kinds.** Kind rules apply where no key or reference does (to columns the SQL type made text, number or
timestamp). The first matching rule wins, project rules before preset rules (`EffectiveRules::column_kind_rule`,
`meta_kind_rule`). `columns` globs match the
table's name without prefix (case-sensitively) and the column name (case-insensitively,
`condition::column_glob_match`); `meta_keys` globs match the key of a
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

**Value styles** apply when switched on: a project style when `enabled`, a preset style as it ships unless the
project lists its id (`enabled` for one that ships off, `disabled` for one that ships on;
`Project::preset_style_on`). The path: the project's own styles, then the applied presets' switched-on styles
(last applied first), each replaced by its project override, become `EffectiveRules.styles`
(`EffectiveRules::add_styles`);
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

## The readout

`readout(session_id, database)` (`src-tauri/src/readout/`) is the home page of a database
(`databee://<slug>/<db>/`), built from the effective rules, the saved queries and the project:

| Part | From |
|---|---|
| `status` | host (or socket), slug, database, prefix, applied presets, server version; `values`: saved queries pinned with `show = "value"`, the first column of their first row as a `Value` |
| `anomalies` | each applied preset's `[[anomaly]]` whose value is a non-zero number (or whose query failed), with `count`, `text`, the `address` `open` names (a saved query, or a table address with the `where` as `filter`) and `fix` (the `scope = "table"` action of the same preset and the table address to plan it on) |
| `recentlyCreated` | one timeline of the 20 newest rows over every entity with a `created` column, newest first, each row once: each entity's newest 20 by `created` (its `filter` applies; one query each, kept as `entities[].sql` with `countInWindow` and `error`), merged by moment (zero dates last); each item is the row's address, the entity its document shows, its `created` value and its [summary](#entity) |
| `watched` | the project's [watched values](#watched-values) as they are now, in the order they were added: each a `WatchedLine { id, label, labelled, source, path, address, value, tree, missing, error }` |
| `pinned` | saved queries pinned with `show = "count"` (`COUNT(*)` of the query) or `"rows"` (the query with `LIMIT <limit>`, 5 by default, unless it has a `LIMIT`); each line carries `tables`, the tables its SQL names after `FROM` or `JOIN` (`db::lexer::read_tables`), and its title lights the first one in the sidebar |
| `recent` | the project's recent objects (`store::recent_objects`), newest first: each row address with when it was opened and its summary |

Every part except `status` and `recentlyCreated` is a `Part { items, error }`; `status.values` is a `Part`,
and `recentlyCreated` is a `RecentlyCreated { items, entities, error }`. Each part runs with its own timeout (10 s for the part, each query at most
5 s or the query timeout when lower), and a part or a line whose query fails carries the error while the rest
of the readout stands. Saved-query SQL runs as one statement on the metadata pool after `USE <database>`, so
its unqualified table names resolve in the readout's database; its columns get kinds by origin as query
results do. Rule SQL (anomalies, entity filters) is qualified with the database.

`remember_object(session_id, address)` records a row address as the newest recent object of its project
(`readout::remember` into `RecentObjectsStore`, `recent_objects.json` keyed by `<slug>/<database>`: at most 20,
each once, with the time in Unix seconds; a slug rename moves them). The page calls it whenever it opens an
object. Recent objects are no rules: projects and their exports do not hold them.

### Watched values

A watched value is one column of one row, or the node at a path in its decoded PHP or JSON value, that the
readout shows as it is stored now (`readout/watched.rs`). Watches are personal: they live in the project's
`watched` (`rules/project.rs`, `WatchedValue`), move with a slug rename and are left out of exports.

- **Adding.** `readout_watch(session_id, database, source, path)` takes the value's provenance (table, column,
  primary key) and the child keys from the decoded value's root as the value tree shows them (`decoded` steps
  into a serialized string; empty: the whole cell). The row is stored by the table's unique key with the
  fewest columns other than the primary key when the row holds a value in each of its columns (`identity`;
  `wp_options` rows by `option_name`, so a deleted and re-added option is still found), else by its primary key.
  Watching a value that is watched already keeps that watch (`ProjectsStore::watch`). An unknown table or
  column is refused.
- **Naming.** The label defaults to `<table>.<column>` and ` › <key>` per path step (`WatchedValue::label`);
  `readout_watch_label(session_id, database, id, label)` stores the user's, a blank or `null` label goes back to
  the default. `readout_unwatch(session_id, database, id)` removes a watch; an unknown id is refused.
- **Reading.** `lines` reads each table the watches name with one query: `SELECT *` with the condition of every
  watched row ORed together (at most 100 rows). A row matches by the stored key values, exactly, else ignoring
  ASCII case as a case-insensitive collation matched it. The cell comes with its kind and style as the table's
  values do (sourced from the row's primary key); a path is looked up in its decoded value by key, the first
  child with that key per step. A leaf becomes a value of its own (int and float numbers, an empty string
  `Empty`, null `NULL`, anything else text) with the cell's provenance; an array or object at the path comes back
  as `tree`, a `DecodedValue` of that node. `address` opens the row.
- **Missing.** Nothing is hidden: a watch whose table, column, row or key is gone, or whose cell is NULL,
  binary or not serialized, says why in `missing`, naming the stored key or column it looked for (`no row
  option_name = 'x' in …`, `no key … in …`); a failed read or a value that does not decode carries `error`.

## `rules_info`

`rules_info(session_id, database)` returns `RulesInfo { prefix, networkPrefix, suggestedPrefix,
appliedPresets, availablePresets, entities, tableEntities, tableIcons }`. `entities` are `{ preset, entity }` with the
real table name; `tableEntities` maps a real table to its entity names; `tableIcons` maps every real table to
its icon ([`[[icon]]`](#icon-table-icons); `table` where no rule gives one). Entities with a `filter` come first
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
| `rows` | `{ unresolved, zeroOrNull }`: the same conditions as WHERE filters on the source table (named by its quoted name, as a table tab's free filter applies them) |

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

The rule behind a reference (for its `when` and `to_when`) is `EffectiveRules::rule_for`: the same `by`, `from`
and `to`, column names compared case-insensitively; the document's incoming groups use the same lookup. The
SQL of conditions is `condition::conditions_sql` (`rules/condition.rs`), shared with incoming groups, action
plans and text search.

A reference from a column to the same column of its table (shared emails, external ids) counts a row as
resolved only when another row matches (by primary key); a table without one finds the row itself.

## Built-in presets

WordPress knowledge is configuration: three built-in presets in `src-tauri/presets/`, written in this format
and governed by [`.claude/rules/presets.md`](../.claude/rules/presets.md). The engine knows no WordPress names.

| Preset | Marker tables | Requires |
|---|---|---|
| `wordpress` | `options`, `posts`, `users` | |
| `woocommerce` | `woocommerce_order_items` | `wordpress` |
| `action-scheduler` | `actionscheduler_actions` | `wordpress` |

Action Scheduler is its own preset because it ships with plugins other than WooCommerce. The prefix is the
shortest one under which a preset's marker tables exist, so multisite sub-sites (no users table) never win;
`users`, `usermeta`, `blogs`, `blogmeta`, `site`, `sitemeta`, `signups` and `registration_log` are network tables
(`{np}`).

**`wordpress.toml`**
- Key/value tables: `postmeta`, `usermeta`, `commentmeta`, `termmeta`, `sitemeta`, `blogmeta` (owned), and
  `options` (standalone, so meta-key kinds apply to option rows). Each owned table declares `fk → owner`.
- References: authors, parents, comments, featured images (`_thumbnail_id`), `_edit_last`, terms, links and
  the network tables. Labelled groups: `children`, `comments`, `replies`, `terms`.
- Entities: `post`, `user` (`people`), `comment`, `term`, `option`, `blog`.
- Kinds: `user_pass`, `user_activation_key` and key globs such as `*password*`, `*api_key*`, `*_token` are
  secrets; `*.*_gmt` and registration dates are UTC timestamps; `post_date`, `post_modified`, `comment_date`,
  `link_updated` are local (zone not stored, shown raw); transient timeouts, `last_update` and `_edit_lock` are
  unix; status, type and taxonomy columns are enums.
- Pinned queries: `site_url`, `db_version`, `autoload_bytes` (readout values), `site_options`,
  `network_plugins`, `content`, `users`, `options_by_prefix`, `options_largest_autoload`, `transients`.
- Anomalies `autoload_heavy` and `transients_expired` (fix: `delete_expired_transients`); actions
  `set_autoload` and `delete_expired_transients`; find source `options`.
- Value style for `posts.post_status = "trash"`, shipped off (`enabled = false`).
- Icons: `*meta` `menu` (every meta table, plugins' included), `users` `user`, `posts` `file-text`, `comments`
  `message-square`, `terms` `tag`, `term_taxonomy` and `term_relationships` `tags`, `options` `settings`,
  `links` `link`, `blogs` and `site` `globe`, `signups` `mail`, `registration_log` `file-clock`.

**`woocommerce.toml`**
- Both order shapes: HPOS (`wc_orders`, its meta, addresses, operational data) and posts (`shop_order*` posts
  with their meta). Whichever holds rows shows up; there is no HPOS or version detection.
- References: customers, parents (`children`), `guest_orders` (same `billing_email`, `customer_id = 0`),
  `addresses`, `items` (to both shapes, with the item meta `_qty`, `_line_total`, `_product_id`,
  `_variation_id` as group columns), `notes` (`comment_type = order_note`), PayPal order ids, subscription
  links, lookup tables, `tokens`, logged-in sessions (`session_key` only when numeric), download permissions.
- Entities: `order` on `wc_orders` and, filtered by `post_type`, on `posts`; `product` (filtered posts),
  `order_item`, `order_address` (`people`), `payment_token`, `session`.
- Kinds: money with the currency of the row (`currency`) or the document (`_order_currency`), amounts without
  a stored currency (item meta, product prices, lookup tables); timestamps (`_date_paid` unix,
  `_paid_date` local, `_schedule_*` UTC, `_wc_*_sent` unix, `_wc_order_attribution_session_start_time` local);
  secrets (`order_key`, payment tokens, API keys); enums (statuses, gateways, item types, log levels).
- Value styles for order statuses, shipped off.
- Patterns: `woocommerce_log` (the order id in the message or context, PayPal and transaction ids, within a day
  before to 90 days after creation) and `actionscheduler_actions` (the order id in `args` or
  `extended_args`), for both order shapes.
- Pinned queries: `woocommerce_version`, `hpos_options`, `orders`, `orders_posts`, `subscriptions`,
  `customers`, `guests`, `guests_posts`, `products`, `sessions`, `hpos_drift`.
- Anomalies `hpos_drift` (only while data sync is on, checked in its SQL) and `log_errors`; actions
  `set_status` (on both shapes, each also updating the other table) and `delete_logs_before`; find sources
  `woocommerce_log` `message`/`context` (contains) and `wc_orders` `transaction_id`/`billing_email` (exact).
- Icons: `wc_orders` `shopping-cart`, `wc_order_addresses` `map-pin`, `wc_order_operational_data` `layers`,
  `woocommerce_order_items` `package`, `woocommerce_payment_tokens` `credit-card`, `woocommerce_sessions`
  `clock`, `wc_customer_lookup` `user-round`, `wc_order_product_lookup` and `wc_product_meta_lookup` `box`,
  `wc_order_stats` `chart-bar`, `woocommerce_downloadable_product_permissions` `key`, `woocommerce_log` `file-clock`;
  the meta tables get `menu` from `wordpress`.

**`action-scheduler.toml`**
- References `group_id`, `claim_id` and `logs`; entity `action`; enums `status` and `hook`; timestamps
  (`log_date_gmt` UTC, `*_local` local; the other `*_gmt` columns are UTC through `wordpress`); value styles
  for `failed` and `complete`, shipped off.
- Pinned queries `actions`, `actions_failed_by_hook`, `actions_overdue`; anomalies `actions_failed`,
  `actions_overdue`; actions `retry` and `cancel`; find source `args`/`extended_args`.
- Icons: `actionscheduler_actions` `clock`, `actionscheduler_logs` `file-clock`, `actionscheduler_groups` `folder`,
  `actionscheduler_claims` `lock`.

To change a preset for one machine, copy it to `<config dir>/presets/` and edit it: a user preset with the same
`name` replaces the built-in ([Where presets live](#where-presets-live)). The pre-preset
`<config dir>/entities.toml` is not read; nothing is converted.

WordPress has no code of its own: no detection, screens or commands. A WordPress database shows the readout,
documents, rows and saved queries like any other, with these rules applied. Removing WordPress support is
deleting the three files and their lines in `BUILT_IN` ([adapting.md](adapting.md#removing-the-wordpress-presets)).

The preset tests (`src-tauri/src/rules/server_tests.rs`, `rules/coverage.rs`, `rules/effective.rs`,
`document/server_tests.rs`, `document/actions/server_tests.rs`, `search/server_tests.rs`) run the built-in files
on the fixtures `wp_test`, `wp_write`, `wp_legacy`, `wp_custom` (prefix `xy_`), `wp_multi` and `databee_test`.
