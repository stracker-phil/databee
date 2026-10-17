# Design language

The reference for how DataBee looks and behaves. Every UX redesign step updates this page when it builds or
changes an item. Items marked "(from U<n>)" are decided but not built yet; the step names are those of the
redesign plan. Decisions: [ADR 0020](adr/0020-everything-is-rows.md) (one workspace),
[ADR 0024](adr/0024-color-schemes.md) (schemes). Content rules for data:
[`.claude/rules/presets.md`](../.claude/rules/presets.md). Implementation (files, registries, CSS
structure): [frontend.md](frontend.md#styling).

## Principles

- Everything is rows. Every object has a Document and a Rows view of the same data; every value renders the
  same way everywhere, decided by its kind, and names where it is stored.
- Mono for every database value (column names and meta keys shown as data included), sans for chrome and
  labels.
- Hierarchy comes from opacity and space, not from size, color or boxes. No pills, badges or cards.
- The accent marks interactive things only: keys, links, focus, selection, the live flash. Nothing else is
  colored unless a value style says so.
- Icons appear in chrome buttons and fields, never in data.

## Surfaces

| Token | Used for |
|---|---|
| `--chrome` | toolbar, tab bar, sidebar, status bar |
| `--bg` | content: grids, documents, the active tab |
| `--pop` | overlays: palette, peek, popovers, context menus, dialogs; clearly brighter than `--bg` |

Separators between surfaces are `--line` hairlines. No inset panels, nested surfaces or tinted cards; the
inset canvas and its tokens (`--bg-panel`, `--bg-surface`, `--shadow-panel`, `--radius-panel`, `--panel-gap`)
are gone. Inside content, bars (the table filter bar, the query action bar, the pending tray) stay on `--bg`
and are separated by hairlines.

## Color schemes and tokens

A scheme is a complete token set in `src/styles/schemes/<id>.css`, applied through
`[data-scheme="<id>"]` on `<html>` (and on the swatches that preview it in Settings). Settings → Appearance
picks one scheme for light and one for dark; the theme setting (system, light, dark) decides which applies.
Built-in: `ayu-mirage` (default dark) and `ayu-light` (default light). No color literal outside a scheme
file; a Vitest scan enforces it.

| Token | Ayu Mirage | Ayu Light | Role |
|---|---|---|---|
| `--chrome` | `#1f2430` | `#eef0f2` | chrome surface |
| `--bg` | `#242936` | `#fcfcfc` | content surface |
| `--pop` | `#323a4c` | `#ffffff` | overlay surface |
| `--field` | `#1a1f29` | `#ffffff` | input background (inset) |
| `--field-line` | `#3b4254` | `#cdd1d6` | input and overlay border, key caps |
| `--text` | `#cccac2` | `#5c6166` | primary text; `--o2`/`--o3` derive from it |
| `--line` | `#2f3545` | `#e6e8eb` | hairlines: rows, separators |
| `--band` | `#2a3040` | `#f2f3f5` | section header band |
| `--row-hover` | `#2b3142` | `#f0f2f5` | hovered row |
| `--shadow` | `0 0 28px rgba(0,0,0,.38)` | `0 0 28px rgba(30,40,60,.16)` | overlay glow, zero offset |
| `--shadow-small` | `0 1px 2px rgba(0,0,0,.25)` | `0 1px 2px rgba(30,40,60,.2)` | selected segment, switch thumb |
| `--shadow-inset` | `inset 0 1px 2px rgba(0,0,0,.15)` | `inset 0 1px 2px rgba(30,40,60,.08)` | field inset |
| `--scrim` | `rgba(10,12,18,.55)` | `rgba(40,50,70,.2)` | behind modal dialogs |
| `--on-accent` | `#1f2430` | `#ffffff` | text on an accent or value-style fill |
| `--knob` | `#cccac2` | `#ffffff` | switch thumb |
| `--a-orange` | `#ffa659` | `#fa8d3e` | accent hue (default) |
| `--a-gold` | `#ffcc66` | `#f2ae49` | accent hue |
| `--a-blue` | `#73d0ff` | `#399ee6` | accent hue |
| `--a-purple` | `#d4bfff` | `#a37acc` | accent hue |
| `--a-graphite` | `#8a919e` | `#8a9199` | accent hue |
| `--style-red` | `#ff6666` | `#e65050` | value-style color |
| `--style-green` | `#87d96c` | `#6cbf43` | value-style color |
| `--style-yellow` | `#ffcc66` | `#eba400` | value-style color |
| `--style-blue` | `#73d0ff` | `#399ee6` | value-style color |
| `--style-purple` | `#dfbfff` | `#a37acc` | value-style color |
| `--code-keyword`, `-string`, `-number`, `-comment`, `-function`, `-type`, `-operator`, `-name`, `-invalid` | Ayu Mirage syntax | Ayu Light syntax | SQL editor syntax colors |
| `--code-cursor`, `-selection`, `-active-line`, `-gutter`, `-match` | per scheme | per scheme | SQL editor chrome |

Derived tokens in `src/styles/theme.css`, the same for every scheme:

| Token | Value | Role |
|---|---|---|
| `--accent` | `var(--a-<choice>)` via `html[data-accent]` | interactive things only |
| `--o2`, `--o3` | `--text` at 62% and 40% | secondary and tertiary text (see opacity levels) |
| `--cell` | `--accent` at 24% | hovered value |
| `--sel` | `--accent` at 20% | selected row or sidebar entry, focused value |
| `--lit` | `--accent` at 15% | linked table highlight |
| `--pad` | 1, 3, 5 or 8px | row padding (see row anatomy) |
| `--row-h` | `20px + 2 × --pad` (the line grows with data sizes above 12.5px) | row height for CSS and the virtualizer |
| `--fs-small`, `--fs-base`, `--fs-large` | 11.5, 13 and 17px in rem | see type |
| `--font-ui`, `--font-mono` | Geist, JetBrains Mono | bundled defaults; Settings accepts any installed family |

Status colors (`--success`, `--warning`, `--danger`, `--info` and their `-soft` variants), `--type-*`,
`--chart-*`, `--category-*` and `--tile-*` are removed. Core components use only the tokens above (an error
or warning background is a `color-mix()` of a `--style-*` color). The WordPress screens still read a few old
names (`--danger`, `--chart-*`, `--category-*`, `--border`, `--text-secondary`, …); they survive as aliases onto
scheme tokens in `theme.css`, marked `/* alias: removed in U2b/U12 */`, until U12 deletes those screens.

**Fonts:** Geist (400, 500, 600) and JetBrains Mono (400, 500, 600, italic 400) are bundled (SIL OFL).
Settings → Appearance "UI font" and "Mono font" take any installed family name; empty means the bundled one.

## Type

| Token | Size | Used for |
|---|---|---|
| `--fs-small` | 11.5px | column header labels, relation text and counts in section bands, captions |
| `--fs-base` | 13px (sans), 12.5px (mono) | everything else |
| `--fs-large` | 17px | the object header (table name and `#key`) only |

Sizes are in rem, so ⌘= and ⌘- scale the whole UI. Line height is 20px at the base size. Weights: 400, 500,
and 600 for the table name in section bands and the object's `#key`. Numbers use tabular figures and are
right-aligned in grids.

## Opacity levels

| Level | Token | Opacity | Used for |
|---|---|---|---|
| Primary | `--text` | 100% | the value you came for |
| Secondary | `--o2` | 62% | keys in key/value rows, units, currency, relation text, relative times, inactive tabs |
| Tertiary | `--o3` | 40% | column types, column header labels, counts, empty states, `NULL`, `""`, struck old values, key caps |

## Row anatomy

Every list shares one row: document rows, grid rows, sidebar items, palette results, Project page lists.

- **Padding:** `--pad` above and below a 20px line. Settings → Appearance "row padding" has four steps
  (1px, 3px, 5px, 8px); the default is step 2 (3px). Grid rows, sidebar items and palette results use it;
  document rows and Project page lists too.
- **Hairline:** every row has a 1px `--line` below it.
- **Row hover:** the whole row takes `--row-hover` (subtle).
- **Cell hover:** the hovered value itself takes `--cell` (stronger), with a 3px radius.
- **Selected cell:** a 1.5px accent outline.
- **No row-number column.** The primary key identifies a row; result sets without one show no number.
- **Kinds without color:** numbers are right-aligned with tabular figures, `NULL` is italic tertiary, binary
  values are tertiary. There are no type tints.
- **Key/value rows** (Document): a 300px key column in `--o2`, the value in primary text.
- **Column header row:** column names in `--o3`, small sans (document groups). Grids (mock Rows)
  show the column name in mono and its type below it in `--o3` small text. Table groups always have one, so
  every value is labelled.
- **Sidebar items** (`.si`): 8px inset, 5px radius, mono, secondary text; hover `--row-hover`; the active
  table `--sel`. History entries use the same row.

## Section header band

Each group in a document starts with a band: `--band` background, `--line` hairlines above and below, 28px
of space above it, 6px 12px inside.

- a disclosure chevron at the left in `--o3`: it, or a click on the band outside its buttons, collapses the
  group to its band (name, relation, count stay; rows, `WHERE` and grid go). Collapsed groups are remembered
  per connection and database for every document of the same table;
- table name: mono, semibold, primary text (sans for `events` and `changes`);
- relation in secondary small text, mirroring the group's link: `this row`, `meta · order_id`,
  `items · order_id → this row`, `pattern · <rule as SQL>`;
- count right-aligned in tertiary small text (`13 columns`, `9 keys · id order`, `2 rows`);
- capped groups end with `+n keys` / `+n rows` in the accent, which open the Rows view at that group; groups
  with 0 rows collapse into one tertiary "Also linked:" line of table names (each opens its rows);
- hovering a band shows the rule's origin (`preset …`, `foreign key …`, `inferred · same name`) in the status
  bar; clicking the table name opens the group's rows in a table tab.

The object header is its own block above the groups: table name (`--o2`, large mono), `#key` (accent,
semibold), the Document/Rows segmented control with its `Tab` key cap, the summary as `key value` pairs,
closed by a hairline. A `⋯` button and a right-click on the block open the row's actions (`Run <action>`,
`(no undo)` when it cannot be undone). In the Document view the focused value has a 1.5px accent outline on a `--sel` background.

**Rows view:** per group the same band, the group's `WHERE` in tertiary mono, then a compact grid (grid header
of U2b: column name, type below) with the table's full columns, 12 rows high before it scrolls.

**Linked table highlight:** hovering a table in the sidebar lights every band of that table
(`--lit` background, 3px accent bar on the left, accent table name); hovering a band lights its sidebar
entry (`--lit` and a 1px accent outline). Hovering a value lights only its provenance table in the sidebar.

## Inputs

Inputs look like inputs, never like rows.

- **Field:** `--field` background, 1px `--field-line` border, 6px radius, `--shadow-inset`, a leading
  icon, placeholder in `--o3`, optional key cap on the right (`InsetField` in `components/Field.tsx`). The
  toolbar search field ("Order ID, email, PayPal ID, table…", `⌘K`) opens the palette; the sidebar filter
  carries `⌘⇧F`, the table filter bar `⌘F`. Plain `.input` controls (dialogs, Settings) share the frame
  without the icon. Key caps follow the user's bindings.
- **Segmented control** (`.segmented`): a field-styled track with 1px padding; segments in `--o2`; the
  selected segment on `--pop` with primary text and `--shadow-small`. Used for Settings choices, the
  Tables/History switch, the mode switch and Document/Rows.
- **Key cap:** mono 11px, `--o3`, 1px `--field-line` border, 4px radius.

## Overlays

Palette, peek, popovers, context menus, toasts and dialogs: `--pop` background, 1px `--field-line` border,
10px radius, `--shadow` (a glow with zero offset, no 3D lift). Modal dialogs and the palette sit over
`--scrim`. Overlays take `Esc` first. Palette results use the row anatomy; the active one is `--sel` with a
2px accent bar on the left, and command shortcuts show as key caps.

## Value grammar

One component (`src/components/value/Value.tsx`) renders every value: grids and the WordPress column fields
today, documents, peek, the palette, the readout and the Project page as they are built. Hover never
opens a tooltip: the status bar shows the provenance of the hovered value, else of the focused (selected)
value, else the current address. The Hover column is what the status bar shows. Built in U5 except where a
cell says otherwise.

| Kind | Look | Click | Hover |
|---|---|---|---|
| PrimaryKey `#3297` | accent, mono | opens the object (Rows view when already in its Document, and back; in an object tab another row's key opens as the next column); in grids double-click and Enter open it too | provenance |
| Reference, declared `customer_id 4` | accent, dotted underline | opens the target: in an object tab as the next column of its stack, elsewhere in the preview tab | provenance, `→` target and source (`foreign key …`, `project …`, `preset …`); ⌥ peeks |
| Reference, inferred | accent, dashed underline, tertiary "inferred" | as declared; several targets: a menu to choose | `→` target `· inferred · same name · indexed` |
| Reference, dangling | tertiary, struck through | none | "no row `#4` in `wp_users`" |
| No reference (`0` or `NULL` in a reference column) | tertiary, no underline | none | "0 = no reference" |
| Enum (status slugs, gateways) `wc-failed` | raw string, default color | filters rows by this value | provenance |
| Number | tabular figures, right-aligned in grids | none | provenance |
| Money | amount primary, currency secondary (`95.40 EUR`) | none | both sources |
| Timestamp | raw value primary, relative time secondary (`1792167625  2 h ago`) | none | UTC and local time |
| Null | italic tertiary `NULL` | none | provenance |
| Empty | tertiary `""` (never rendered like `NULL`) | none | provenance |
| Serialized | tertiary preview `▸ a:17{…}` / `{…17}`; in grids followed by the raw text, tertiary | expands inline as a tree (grids: opens a value column) | provenance |
| Secret | masked | reveals | provenance |
| Binary | tertiary `<n bytes>` | none | provenance |

- **Trees** (`ValueTree`): a hairline guide on the left, keys secondary, container types tertiary
  (`array(2)`), strings in dim quotes so `"2.04"` and `2.04` differ, leaf pairs of a small array (up to 4
  leaves, 60 characters) on one line unless the tree is editable. Tree rows keep the row anatomy.
- **Value styles (from U4c):** a project (or preset, off by default) rule `{ columns or meta keys, match:
  = | LIKE | REGEXP, value, color }` colors matching values with `--style-<color>` (`red`, `green`, `yellow`,
  `blue`, `purple`). A style is always scoped to columns or meta keys, never global. There is no status kind
  and no default status color.
- **Provenance text:** `wp_wc_orders_meta.meta_value · id 9123`, plus `→ wp_users #12 · preset woocommerce`
  for references, `UTC … · local …` for timestamps with a known zone (`zone not stored` otherwise; zero
  dates have neither) and `currency <source>` for money. Query results carry no row, so only the kind's
  part shows.
- **Secondary text** (`Value.secondary`, a relative time, a currency, a formatted amount) follows the raw
  value in `--o2`, never instead of it.
- **Change marks (from U13):** a changed value flashes (`--cell` fading out over 1.6s), then shows the old
  value tertiary and struck through, with a 3px accent bar on the row.

## Layout

- **Toolbar** (`--chrome`): connection and database as plain mono text (`wc-pp / db`; a click opens the
  connection or database picker), the path of the column stack (`order #4711 › user #12 › payment_token #7`),
  the search field on the right, then icon buttons. The WordPress/Database mode switch stays as a
  segmented control until U12.
- **Tab bar** (`--chrome`): tabs are cells separated by `--line`; the active tab takes `--bg` and joins the
  content; preview tabs show italic titles. Tabs: query, table, object, project.
- **Sidebar** (`--chrome`): the Tables/History switch, the filter field, a `db · N tables` caption, then the
  database tree. `view.sidebar` (⌘\, the palette, or the ▤ button) hides it entirely (no rail); the state
  is persisted per connection (`sidebarHidden` in `ui_state.json`).
- **Status bar** (`--chrome`), left to right: ▤ sidebar toggle, the active tab's address in mono
  (`databee://wc-pp/db/wp_posts?post_type=page`; a click copies it; `· filtered` follows when a typed SQL
  filter applies, which no address holds; replaced by the provenance of the hovered or selected value), for a
  query tab its result summary after it,
  spacer, `presets wordpress · woocommerce` (the presets that apply to the current database; a click opens its
  Project page), `pending n` (a button to the tray when n > 0), `● live 2 s` in the accent while live
  (from U13). Plain text, no chips; the theme lives in Settings and the palette.
- **Column stack:** an object tab is a stack of columns. Enter or a click on a reference (or on another row's
  key) opens its target as a new column right of the column it is in, closing any columns to the right of
  it. Columns are at least 420px wide; when they don't fit, the oldest collapse to 34px strips with a
  vertical `table #key` label (accent key), and a click on a strip pops back to it. The active column has a 2px
  accent line on top; `h`/`l` move it, a click in a column activates it. Back and forward restore the stack
  each address had.
- **Path** in the toolbar, after the connection: the stack as crumbs, `order #4711 › user #12 › payment_token
  #7`, mono, the name in `--o2` (the entity name the rules give the row, else the table name) and the key of the
  active column in the accent. Every crumb but the last is a button that pops back to its column.
- **Value column:** Enter on a serialized value, or Enter in a grid cell with serialized data or long text,
  opens the value as a column: in an object tab the next column of the stack, in table and query tabs a
  column on the right of the tab (at least 420px). Its head names `table.column` and the row key, offers
  PHP/JSON or Raw, copy and close; the tree edits leaves (`edit_serialized`, queued as a cell edit through the
  pending tray) when the value's row has a single-column key. There is no docked inspector.
- **Peek:** holding ⌥ over a reference, or with a reference focused, shows a `--pop` overlay below it: head
  `table #key` with "⌥ peek · ↵ open", the target's header values (its entity's summary, else its first five
  non-key columns) as key/value rows, footer `via customer_id → wp_users.ID · preset woocommerce`. Releasing ⌥
  hides it; ↵ while it shows opens the target like Enter.
- **Shortcut overview:** `?` opens a dialog listing the bound commands of the scopes active at that moment,
  most specific scope first, with their key caps; `?` or Esc closes it.

## Project page

A `project` tab (`databee://<slug>/<db>/?project`; the `project.open` command, palette `>Project`; the status
bar's preset text) shows the rules of one database as section bands with rows, in this order: presets and
prefix (prefix field with `suggested: wp_`, a switch per preset with its marker tables in `--o3`, "Export as
preset…" to a save panel), the rule form when one is open, references, value styles, kinds. Details and
behavior: [presets.md](presets.md#the-project-page).

- **References:** columns source, when, target, origin, coverage, actions; mono rows with the row anatomy.
  Project rows in primary text (edit, remove); preset and foreign-key rows in `--o2`, read-only (preset:
  disable); inferred rows with a dashed target and `same name · indexed` / `not indexed` in `--o3` (promote,
  disable); switched-off rows in `--o3`, struck through (enable). Coverage is on demand: "check" per row,
  "Check coverage for all" in the band; it reads `6 of 7 · 1 is 0 or NULL`, `running…`, `not checked`,
  `timed out`.
- **Rule form:** a band `new reference · from r on <table>.<column> · <key>`, then labelled fields (source and
  target as table and column selects, `when` as column `=` value, label), the live coverage line in mono
  (`0 of 1 resolve`) with the warning "no source value matches a target; the rule is probably wrong ·
  unresolved: …" in `--o2`, then Cancel (`--o2`) and Save to project (accent). A Rust error shows in
  `--style-red` small text under the field it names.
- **Value styles:** column or meta key, match (`= 'wc-failed'`), color (its name in its `--style-*` color;
  the form picks from swatches of the scheme's style colors), origin. None by default.
- **Kinds:** column or meta key, kind (`money · currency from currency`, `timestamp · unix`), origin.
- Actions are plain accent text buttons separated by ` · `; no chips or icons.

## Keys

Defaults; every key is a rebindable command (Settings → Shortcuts, `keybindings.json`). Bare keys fire only in
the object and grid scopes, never in inputs, the editor or dialogs; `Tab` keeps its normal meaning there. The
column keys act only while the keyboard focus is in the stack (or nowhere), so the sidebar and the tab bar
keep their arrow keys. A key and its arrow are two commands (`object.down`, `object.downArrow`), each
rebindable.

| Key | Scope | Action |
|---|---|---|
| `Tab` | object | Document ↔ Rows of the active column; focus stays on the same provenance |
| `j` / `k`, ↓ / ↑ | object, grid | move the focused value in a Document column (focus ring `--sel` and accent outline; the status bar shows its provenance when nothing is hovered); grids keep their arrow keys |
| `h` / `l`, ← / → | object | move the active column (a focused grid keeps ← / →) |
| `Enter` | object, grid | drill into the focused reference or another row's key (next column), open serialized data as a value column; grids: open the row as an object, a reference's target, a value column for serialized data or long text |
| `Esc` | object, table, query | close the rightmost column (a stack's or a tab's value column); overlays, menus and inputs take `Esc` first |
| ⌘[ / ⌘] | global | back / forward through the active tab's address history (WordPress mode: its own history until U12); without history the keys stay with the editor |
| `⌥` (held) | object, grid | peek the hovered or focused reference; ↵ opens it |
| `y` / `Y` / `e` / `f` | object, grid | copy raw / copy as SQL condition / edit / filter by value (grid: the `table` scope; documents and Rows grids: the `object` scope) |
| `r` | object, grid | add a reference from this column or meta key: opens the Project page with the form prefilled (context menu: "Add reference…", "Kind…", "Style…") |
| `w` | object | live mode on/off (from U13) |
| `s` | object | snapshot (from U13) |
| `x` | object | dismiss change marks in the focused column (from U13) |
| `p` | query, table | pin the current query to the readout (from U11) |
| ⌘\ | global | toggle the sidebar |
| ⌘⇧F | db | focus the sidebar filter |
| ⌘K | global | palette |
| `?` | global | shortcut overview |

## Palette prefixes

The palette is the main navigation. Results render with the value grammar (from U10). Enter opens in the
current tab (the object tab's column stack from U10); ⌘↵ opens a new tab.

| Input | Result |
|---|---|
| `>` | commands |
| `t:orders` | tables |
| `#3297` | every object with that id, each with its table name (today: WordPress entities; any rules-defined entity, and rows that reference the id, from U10) |
| `@john` | entities whose rule marks them `people = true`, searched on their `search` columns (today: WordPress entities; rule-driven from U10) |
| text of 6+ characters | values in `search` columns and non-key reference targets (external ids) (from U10) |
| `q:` | saved and pinned queries (from U11) |
| `=value` | find everywhere (from U10) |
| `databee://…` | open the address (from U10) |
