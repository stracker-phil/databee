# Design language

The reference for how DataBee looks and behaves. Update this page with every change that builds or changes
an item. Decisions: [ADR 0020](adr/0020-everything-is-rows.md) (one workspace),
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
or warning background is a `color-mix()` of a `--style-*` color). Older rules in `app.css` still read a few old
names (`--text-tertiary`, `--chart-green`, `--chart-orange`); they survive as aliases onto scheme tokens in
`theme.css`. New CSS uses only the tokens above.

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
- **Selected cell:** a 1.5px accent outline. While its grid has the keyboard focus it also takes `--sel`, like the Document view's
  focused value; the grid itself draws no focus outline (a focused grid without a selected cell underlines
  its header in the accent).
- **No row-number column.** The primary key identifies a row; result sets without one show no number.
- **Kinds without color:** numbers are right-aligned with tabular figures, `NULL` is italic tertiary, binary
  values are tertiary. There are no type tints.
- **Key/value rows** (Document): a 300px key column in `--o2`, the value in primary text.
- **Column header row:** column names in `--o3`, small sans (document groups). Grids
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
semibold), the Document/Rows segmented control with a `⌘1` / `⌘2` key cap on each segment, the [summary line](#summary-line),
closed by a hairline. An `Actions` button (only when the row has actions) and a right-click on the block open
the row's actions (`Run <action>`, `(no undo)` when it cannot be undone), each with a tertiary second line of
what it writes (`ObjectAction.writes`, e.g. `wp_wc_orders: date_updated_gmt, status · wp_posts: post_status`;
`deletes rows` or `free SQL` for those). Menu items take that line through `MenuEntry.detail` (`ContextMenu.tsx`).
Running an action opens the action sheet (a modal like the palette, `ActionSheet.tsx`): the action name
(mono), `table #key · preset`, the writes line, a field per param, then every row it writes as
`table #key` with one `column  old → new` line per set column (raw values, mono column names, the arrow
tertiary), deleted rows as `table #key deleted`, or for free SQL "affects rows not known in advance" and
its statements. It previews again 250 ms after a param changes. Below: `<n> rows · undo available` or
`no undo`, and that the change is queued for review in the pending tray before `⌘S`. Buttons: Cancel and
Queue change (accent, disabled while the preview loads or failed; a failure shows in red). In the Document view the focused value has a 1.5px accent outline on a `--sel` background.

**Rows view:** per group the same band, the group's `WHERE` in tertiary mono, then a compact grid (grid header:
column name, type below) with the table's full columns, 12 rows high before it scrolls.

**Linked table highlight:** hovering a table in the sidebar lights every band of that table
(`--lit` background, 3px accent bar on the left, accent table name); hovering a band lights its sidebar
entry (`--lit` and a 1px accent outline). Hovering a value lights only its provenance table in the sidebar; hovering
anywhere else on a row of a known table (Document, grid, readout, palette hit) does the same for its table.

## Inputs

Inputs look like inputs, never like rows.

- **Field:** `--field` background, 1px `--field-line` border, 6px radius, `--shadow-inset`, a leading
  icon, placeholder in `--o3`, optional key cap on the right (`InsetField` in `components/Field.tsx`). The
  toolbar search field ("Order ID, email, PayPal ID, table…", `⌘K`) opens the palette; the sidebar filter
  carries `⌘⇧F`, the table filter bar `⌘F`. Plain `.input` controls (dialogs, Settings) share the frame
  without the icon. Key caps follow the user's bindings.
- **Segmented control** (`.segmented`): a field-styled track with 1px padding; segments in `--o2`; the
  selected segment on `--pop` with primary text and `--shadow-small`. Used for Settings choices, the
  Tables/History switch, Data/Structure in table tabs, the query results picker, PHP/JSON/Raw in value
  columns and Document/Rows.
- **Key cap:** mono 11px, `--o3`, 1px `--field-line` border, 4px radius.

## Overlays

Palette, peek, popovers, context menus, toasts and dialogs: `--pop` background, 1px `--field-line` border,
10px radius, `--shadow` (a glow with zero offset, no 3D lift). Modal dialogs and the palette sit over
`--scrim`. Overlays take `Esc` first. Palette results use the row anatomy; the active one is `--sel` with a
2px accent bar on the left, and command shortcuts show as key caps. Context menus use the UI font wherever they
open (grid, document, sidebar); the hovered or keyboard-focused item is `--accent` with `--on-accent` text,
disabled items `--o3`, separators a 1px `--line`.

## Value grammar

One component (`src/components/value/Value.tsx`) renders every value: grids, documents, peek, the
palette, the readout and the Project page. Hover never
opens a tooltip: the status bar shows the provenance of the hovered value, else of the focused (selected)
value, else the current address. The Hover column is what the status bar shows.

| Kind | Look | Click | Hover |
|---|---|---|---|
| PrimaryKey `#3297` | accent, mono | opens the object (Rows view when already in its Document, and back; in an object tab another row's key opens as the next column); in grids double-click and Enter open it too | provenance |
| Reference, declared `customer_id 4` | accent, dotted underline; icons on hover (below) | opens the target: in an object tab as the next column of its stack, in a table tab in place (the tab becomes the target's object tab; back returns), elsewhere in the preview tab | provenance, `→` target and source (`foreign key …`, `project …`, `preset …`); ⌥ peeks |
| Reference, inferred | accent, dashed underline, tertiary "inferred"; icons on hover | as declared; several targets: a menu to choose | `→` target `· inferred · same name · indexed` |
| Reference, dangling | tertiary, struck through | none | "no row `#4` in `wp_users`" |
| No reference (`0` or `NULL` in a reference column) | tertiary, no underline | none | "0 = no reference" |
| Enum (status slugs, gateways) `wc-failed` | raw string, default color | filters rows by this value | provenance |
| Number | tabular figures, right-aligned in grids | none | provenance |
| Money | amount primary, currency secondary (`95.40 EUR`) | none | both sources |
| Timestamp | raw value primary, relative time secondary (`1792167625  2 h ago`) | none | UTC and local time |
| Null | italic tertiary `NULL` | none | provenance |
| Empty | tertiary `""` (never rendered like `NULL`) | none | provenance |
| Serialized | tertiary preview `▸ a:17{…}` / `{…17}`; in grids followed by the raw text, tertiary | expands inline as a tree (grids: opens a value column) | provenance |
| Secret | masked | reveals until it loses focus | provenance; ⌥ held alone reveals it while hovered |
| Binary | tertiary `<n bytes>` | none | provenance |

- **Reference icons:** while its cell (grids) or line (Document, readout) is hovered, or the cell or field is
  focused, a reference that acts on click shows icon buttons at the right edge, over a fade of the background
  (the text is cut before them; nothing shifts): ↗ (external link) **goes to** the target, moving the whole
  tab there (`Go to (replaces this view)`); › (chevron) **opens it in the side panel**, the next column of the
  stack. In an object tab both show; › is the click. In a table tab both show (› only for rows with a
  single-column key): ↗ is the click, › moves the tab to the reference's own row with the target as the next
  column. Elsewhere (query results, readout) only ↗, which opens the preview tab like the click. Several
  targets: the menu first, then the icon's mode. Dangling references have no icons.
- **Trees** (`ValueTree`): a hairline guide on the left, keys secondary, container types tertiary
  (`array(2)`), strings in dim quotes so `"2.04"` and `2.04` differ, leaf pairs of a small array (up to 4
  leaves, 60 characters) on one line unless the tree is editable. Tree rows keep the row anatomy.
- **Value styles:** a project (or preset, off by default) rule `{ columns or meta keys, match:
  = | LIKE | REGEXP, value, color }` colors matching values with `--style-<color>` (`red`, `green`, `yellow`,
  `blue`, `purple`). A style is always scoped to columns or meta keys, never global. There is no status kind
  and no default status color.
- **Provenance text:** `wp_wc_orders_meta.meta_value · id 9123`, plus `→ wp_users #12 · preset woocommerce`
  for references, `UTC … · local …` for timestamps with a known zone (`zone not stored` otherwise; zero
  dates have neither) and `currency <source>` for money. Query results carry no row, so only the kind's
  part shows.
- **Secondary text** (`Value.secondary`, a relative time, a currency, a formatted amount) follows the raw
  value in `--o2`, never instead of it.
- **Change marks:** a changed value flashes (`--cell` fading out over 1.6s), then shows the old
  value tertiary and struck through, with a 3px accent bar on the row ([Live mode](#live-mode-snapshots-compare)).

## Layout

- **Toolbar** (`--chrome`): connection and database as plain mono text (`wc-pp / db`; a click opens the
  connection or database picker), back and forward (‹ ›, for a table or object tab, disabled without history),
  the path (`wp_wc_orders › order #4711 › user #12 › payment_token #7`, a table tab's table),
  the search field on the right, then icon buttons. There is no mode switch: WordPress is a preset.
- **Tab bar** (`--chrome`): tabs are cells separated by `--line`; the active tab takes `--bg` and joins the
  content; preview tabs show italic titles. Tabs: readout (first, one per connection), query, table, object,
  project.
- **Sidebar** (`--chrome`): the Tables/History switch, the filter field, a `db · N tables` caption, then the
  database tree. `view.sidebar` (⌘\, the palette, or the ▤ button) hides it entirely (no rail); the state
  is persisted per connection (`sidebarHidden` in `ui_state.json`).
- **Status bar** (`--chrome`), left to right: ▤ sidebar toggle, the active tab's address in mono
  (`databee://wc-pp/db/wp_posts?post_type=page`; a click copies it; `· filtered` follows when a typed SQL
  filter applies, which no address holds; replaced by the provenance of the hovered or selected value), for a
  query tab its result summary after it,
  spacer, `presets wordpress · woocommerce` (the presets that apply to the current database; a click opens its
  Project page), `pending n` (a button to the tray when n > 0), `● live 2 s` in the accent while the
  active column is live (a click ends it). Plain text, no chips; the theme lives in Settings and the palette.
- **Column stack:** an object tab is a stack of columns. Enter or a click on a reference (or on another row's
  key, or the reference's › icon) opens its target as a new column right of the column it is in, closing any
  columns to the right of it; the ↗ icon moves the whole tab to the target instead. Columns are at least 420px wide; when they don't fit, the oldest collapse to 34px strips with a
  vertical `table #key` label (accent key), and a click on a strip pops back to it. The active column has a 2px
  accent line on top; `h`/`l` move it, a click in a column activates it. Every column after the first has a
  close button (×) in its header, which pops back to the column left of it. Back and forward restore the stack
  each address had.
- **Table and object in one tab:** an object belongs to its table. The table name in an object header and the
  first crumb of the path show the whole table in the same tab (where its structure is too); back returns to
  the object with its columns. A table tab's history and an object tab's history are one: a tab moves between
  table and row in place, keeping its place in the tab bar.
- **Path** in the toolbar, after back and forward: the root row's table, then the stack as crumbs,
  `wp_wc_orders › order #4711 › user #12 › payment_token #7`, mono, the name in `--o2` (the entity name the
  rules give the row, else the table name) and the key of the active column in the accent. The table crumb
  shows the whole table in the tab; every other crumb but the last is a button that pops back to its column. In a
  table tab the path is the table name, with `(filtered)` while a filter applies.
- **Value column:** Enter on a serialized value, or Enter in a grid cell with serialized data or long text,
  opens the value as a column: in an object tab the next column of the stack, in table and query tabs a
  column on the right of the tab (at least 420px). Its head names `table.column` and the row key, offers
  PHP/JSON or Raw, copy and close; the tree edits leaves (`edit_serialized`, queued as a cell edit through the
  pending tray) when the value's row has a single-column key. There is no docked inspector.
- **Peek:** holding ⌥ over a reference, or with a reference focused, shows a `--pop` overlay below it: head
  `table #key` with "⌥ peek · ↵ open", the target's [summary](#summary-line) (wrapping), footer
  `via customer_id → wp_users.ID · preset woocommerce`. A reference to a set (a table address, e.g. a guest
  order's `billing_email`) heads the card `wp_wc_orders · billing_email = x` and lists "N rows match" with up to
  5 rows (`#key` and summary, each opens its row on click), "N more · ↵ opens all" beyond them, and "no row
  matches" for none. The row the reference is stored in is left out of the set and the count ("N other rows
  match", "no other row matches"). Releasing ⌥ hides it; ↵ while it shows opens the target like Enter.
- **Shortcut overview:** `?` opens a dialog listing the bound commands of the scopes active at that moment,
  most specific scope first, with their key caps; `?` or Esc closes it.

## Project page

A `project` tab (`databee://<slug>/<db>/?project`; the `project.open` command, palette `>Project`; the status
bar's preset text) answers two questions first: what did I change about the presets, and what looks
broken. Every rule per table sits one click further. Details and behavior:
[presets.md](presets.md#the-project-page). The patterns: VS Code's "Modified" settings with reset, Xcode's
build-setting levels, a Problems panel.

- **Header:** one compact row in `--fs-small`: `presets` in `--o2`, a switch per preset (marker tables in its
  tooltip), `auto` in `--o3` or "back to auto", the prefix field with `suggested: wp_` in `--o3`, "Export as
  preset…" (a save panel), and on the right a segmented control **Overview | All rules**.
- **Overview**, two bands and a link:
  - **Your changes:** only the project's deviations from the presets, one row each: what in UI type and
    `--o2` (`switched off reference · preset woocommerce · guest_orders`), the rule in mono with real names
    (a link to the table's Structure view; `· was …`, `· suggested …`, `· auto …`, `· off` in `--o3`), then
    edit and the undo ("reset", "remove", "switch off"). Empty: "No changes — this database uses the presets
    as shipped."
  - **Problems:** coverage counts on its own (`counting n of m references…` in the band, "Refresh
    coverage" on the right) but only problems show: `wp_woocommerce_order_items.order_id → wp_posts.ID: 1 of 3
    point nowhere (4713) · preset woocommerce` with "Open rows" (the table filtered to those rows) and "Open table"; failing
    anomaly queries too. Empty: "No problems found."
  - "All rules…" opens the per-table listing.
- **All rules:** a mono field "filter tables" with `n of m tables` in `--o3` and "Refresh coverage", then
  **tables:** a section band per table (chevron, mono table name, counts on the right), collapsible and
  remembered like document groups. Inside, small `--o3` captions (references, value styles, kinds) over mono
  rows with the row anatomy:
  - **References:** source column (`when …` in `--o3`), `→ target` (`where …`), origin in UI type
    (`project`, `preset woocommerce`, `inferred · same name · indexed`, `foreign key …`, plus `· edited in
    project`), coverage, a small switch, actions. Project and edited rows in primary text, preset and
    foreign-key rows in `--o2`, inferred targets dashed, switched-off rows in `--o3` and struck through.
    Coverage counts on its own: `counting…`, then a 3rem bar (resolved in `--o2`, 0 or NULL lighter) and
    `6 of 7`; `1 is 0 or NULL · unresolved 9` is the tooltip; `timed out`, or `off` / `not applied`.
  - A **referenced by** line in `--o2`: the sources pointing at the table, struck through when off.
  - **Value styles:** column or meta key, match (`= 'wc-failed'`), color (its name in its `--style-*` color),
    origin, switch, actions. None by default.
  - **Kinds:** column or meta key, kind (`money · currency from currency`, `timestamp · unix`), origin.
  - **Entities and actions:** read-only `--o2` lines `entity order where … · preset woocommerce`,
    `action mark_paid · row · preset woocommerce`.
- **Rule sheet:** forms open in a modal sheet styled like the palette (same width, pop animation): a head
  with the title (`edit reference`) and `from <opener>` in mono `--o3`, labelled fields in two columns
  (source and target as table and column selects, `when` as column `=` value, label), a foot with the live
  coverage line (`0 of 1 resolve`, the warning "no source value matches a target; the rule is probably wrong ·
  unresolved: …" in `--o2`), Cancel `⎋` (`--o2`) and Save to project `⌘↵` (accent). A Rust error shows in
  `--style-red` small text under the field it names. ⎋ closes, ⌘↵ saves.
- Actions are plain accent text buttons separated by ` · `: edit, reset (an edited preset or inferred rule),
  remove (a project rule); no chips or icons.

## Structure view

A table tab's Structure view lists one row per column with everything that applies to it, in the
Project page's language. Details: [presets.md](presets.md#the-structure-view).

- **Columns grid:** `--fs-small` `--o2` headers (name, type, keys, kind, references, value styles), then a row
  per column: the mono name in semibold, the mono type over a small `--o3` line `nullable · default NULL` (or `not null`, extras like `auto_increment`, the comment),
  key marks (`PK` badge, `UNI`/`idx` plus the index name and, past the first, the column's position), then
  stacked rule items. Each item is a mono main line (`timestamp · utc`, `→ wp_users.ID` with `(rows with …)` for the target's condition and `when this row has …` for the
  source's in `--o3`; a column referencing itself reads `→ other wp_wc_orders rows with the same billing_email`, `= 'pending' → yellow` in its `--style-*` color) over a small `--o2` meta line: origin in UI type in
  `--o3` (`preset wordpress: *.*_gmt` names the glob that matched), coverage bar, small switch, actions.
  Switched-off rules and overridden kinds are struck through in `--o3`; inferred targets are dashed.
- **Column menu:** right-click or the `⋯` button: "Add reference…", "Add value style…", "Add kind…",
  separator, "Open rows". The forms open in the Project page's rule sheet.
- **Blocks below:** `--fs-small` uppercase `--o2` headings: "meta keys · <meta table> (key → value)" with a
  line per rule (the key condition or scope in mono, then the rule item), "Referenced by" (rule items naming
  the source), "Entities" (name, `where`, `summary`, `created`, preset), "Actions" (name, scope, what it
  writes, preset), then the DDL with its copy button.

## Readout

The readout tab (`databee://<slug>/<db>/`) is the home of a database, opened on connect. It uses the full
width of the tab on `--bg` and reads top to bottom, calm and scannable; SQL is provenance, never inline by
default. A header block like the object view's: the database as the title in mono, one meta line in `--o2`
(host · prefix · presets · server version), then the value-pinned queries as a compact key/value list (query
title | value | source table in `--o3`). Then a collapsible section band per part (chevron, name, hint in
`--o2`, count in `--o3`), remembered per database like the object view's groups:
- **needs attention** (anomalies), only while an anomaly's value is above 0: one card-like row per anomaly
  (`--chrome`, rounded, 1px `--line`): the count large in mono and uncolored | the rule text as stored, with
  `preset · id` in `--o3` below | buttons "Show rows" and, when the preset offers one, the fix action's name
  (mono, accent), which opens the action sheet;
- **recently created**: one timeline of the newest rows across every entity with a `created` column, newest
  first. Above it a chip per entity (entity name and its count in the window; a failed entity says `failed` in
  `--style-red`, its error in the tooltip). A chip toggles its entity in the timeline (all on by default); its
  chevron discloses the entity's table (opens it), preset and SQL. Each line: relative time in `--o3` (the
  stored value and its source column on hover) | entity in `--o2` | `#key` in the accent | the
  [summary](#summary-line);
- **pinned**: count lines (count | title | SQL in `--o3`) and full-width row blocks; the band's hint says how
  to pin;
- **recently opened**: table in `--o2` | `#key` in the accent | the summary | the relative time in `--o3`.

Every line opens its address and lights its table in the sidebar on hover; counts refresh after every apply.
A failed part or line shows its error in `--style-red` in its place; the rest stands.

### Summary line

A row's summary (Rust's one summary rule) renders the same everywhere it shows: the object header, the peek,
the readout's timeline and recently opened rows. Each value is `name value`: the column or meta key in `--o3`,
then the value with its kind (`Value`, grid density); NULL shows as NULL, never dropped.

## Live mode, snapshots, compare

Changes show in the object itself (`.chg`, `.flash`, `.old`, the `changes` band), never in a
separate tool.

- **Live mode** (`w`) reads the active column's row again every few seconds (Settings → Queries → Live
  interval, default 2 s); the status bar shows `● live 2 s` in the accent at its right end.
- **A changed value** flashes once (`--cell` fading to transparent over 1.6 s), then keeps its old value next
  to it in `--o3`, struck through; its row gets a 3px accent mark on the left. A new row gets the same mark,
  and a new meta value shows `no row` struck through. A removed row stays where it was, in `--o3` and struck
  through, until `x` dismisses the marks of the column.
- **The change list** is a section band `changes` at the bottom of the column (`since live started` or
  `since snapshot <label> (2 h ago)`, `in key order within one poll`, the `x` key, the count), then rows of
  time, table, column or meta key (a row's key for a new or removed row) and `old → new` (old in `--o3`;
  `NULL`, `no row`). Lines are in the order they were seen; changes found by one poll have no order of their
  own, so they are listed by table, primary key and column. A line jumps to the value (the Document view, the
  value focused).
- **Snapshots:** `s` stores the active column's row (palette: "Snapshot saved query results…" stores the first
  1,000 rows of a saved query's tab, keyed by primary key). "Changes since snapshot…" (palette) lists the
  snapshots of the row or query, newest first, and draws the same marks and list against the one picked; in a
  saved query's tab the list sits under the results, and a line opens its row.
- **Compare** (`databee://…/<table>/<key>?compare=<key2>`, palette "Compare with another row…"): the column
  shows both rows side by side, each with its header, and marks every value and row that differs on both
  sides; nothing is struck through. A line on top says how many differences there are and how they align:
  groups by rule, meta rows by key, other rows by position.

## Keys

Defaults; every key is a rebindable command (Settings → Shortcuts, `keybindings.json`). Bare keys fire only in
the object and grid scopes, never in inputs, the editor or dialogs. `Tab` is never bound: it moves the keyboard focus everywhere. The
column keys act only while the keyboard focus is in the stack (or nowhere), so the sidebar and the tab bar
keep their arrow keys. A key and its arrow are two commands (`object.down`, `object.downArrow`), each
rebindable.

| Key | Scope | Action |
|---|---|---|
| `⌘1` / `⌘2` | object | Document / Rows of the active column; focus stays on the same provenance (`object.viewDocument`, `object.viewRows`) |
| `j` / `k`, ↓ / ↑ | object | move the focused value in a Document column (focus ring `--sel` and accent outline; the status bar shows its provenance when nothing is hovered); grids keep their arrow keys |
| `h` / `l`, ← / → | object | move the active column (a focused grid keeps ← / →) |
| `Enter` | object | drill into the focused reference or another row's key (next column), open serialized data as a value column (`object.drill`); grids handle Enter themselves: open the row as an object, a reference's target, a value column for serialized data or long text |
| `Esc` | global | close the rightmost column, a stack's or a tab's value column (`columns.close`); overlays, menus and inputs take `Esc` first |
| ⌘[ / ⌘] | global | back / forward through the active tab's address history (also the ‹ › toolbar buttons of table and object tabs); without history the keys stay with the editor |
| `⌥` (held) | object, grid | peek the hovered or focused reference; ↵ opens it (not a command) |
| `y` / `Y` / `e` / `f` | table, object | copy raw / copy as SQL condition / edit / filter by value (table grids: the `table` scope; documents and Rows grids: the `object` scope) |
| `r` | table, object | add a reference from this column or meta key: opens the Project page with the rule sheet prefilled (context menu: "Add reference…", "Kind…", "Style…") |
| `w` | object | live mode on/off for the active column (`object.live`) |
| `s` | object | take a snapshot of the active column's row, after a label prompt (`object.snapshot`) |
| `x` | object | dismiss the change marks of the active column (`object.dismissChanges`) |
| `p` | editor (focus in the results) | pin the tab's saved query: Not pinned, Count, Value or Rows(n) (`query.pin`) |
| ⌘S | global, editor | `changes.apply` applies pending changes; with nothing pending, `query.save` (its fallback) saves the query tab |
| ⌘↵ / ⌘⇧↵ | editor | run the statement at the cursor / all statements |
| ⌘F | table | focus the table's `WHERE` filter |
| ⌘⇧F | db | focus the sidebar filter |
| ⌘K / ⌘P | global | palette / palette with `t:` (tables) |
| ⌘⇧P | global | switch connection or database: the palette with `c:` (`connection.switch`, also File menu) |
| ⌘T / ⌘W | global | new query tab / close tab |
| ⌃Tab / ⌃⇧Tab | db | next / previous tab |
| ⌘R | global | refresh the current view |
| ⌘N / ⌘, | global | new connection / Settings |
| ⌘= / ⌘- / ⌘0 | global | zoom in / out / reset |
| ⌘\ | global | toggle the sidebar |
| `?` | global | shortcut overview |

Scopes are the registry's (`src/commands/index.ts`, `registerCoreScopes`): `global`, `db`, `table` (a table
tab), `editor` (a query tab) and `object` (an object tab).

## Palette prefixes

The palette is the main navigation. Sections, in order: Recent, row hits, saved queries, Connections and Databases (`c:`), Tables, Commands. Row hits are
grouped in sections titled by what they are (`objects with id 4711`, `rows that reference 4711`) and render
with the value grammar: the table name (or `table.column`) in `--o2`, `#key` in the accent, then the summary
values. Enter opens a hit in the current object tab's column stack (or a new object tab); ⌘↵ opens a new
tab; holding ⌥ peeks the selected hit. The footer lists the keys and the prefixes.

| Input | Result |
|---|---|
| `>` | commands |
| `t:orders` | tables |
| `#3297` | every object with that id across the entities, each with its table name, and the rows that reference it (incoming references and patterns). Without entities, the primary keys of every table under 1M estimated rows (the footer says so) |
| `@john` | entities whose rule marks them `people = true`, searched on their `search` and `search_exact` columns and meta keys |
| text of 6+ characters | values in the entities' `search` columns and non-key reference targets (external ids); shorter text matches only tables and commands |
| `q:` | saved queries: the project's, the global ones and the presets' pinned ones, with their origin |
| `c:` | switch connection (⌘⇧P): open sessions (the active one marked `current`, others `open`), then recent connections, each with its name, `host:port / database` and slug; then the active connection's databases (`current` marked). Fuzzy: substring, else the characters in order within one field. Enter activates an open session, connects a recent one (asking for the password when needed; the other sessions stay open), or switches the database |
| `=value` | find everywhere: every `[[meta]]` value, the `[[find]]` sources and the entities' search columns, one section per table and column |
| `databee://…` | open the address |
