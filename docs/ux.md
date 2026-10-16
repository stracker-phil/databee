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

## Surfaces (from U2b)

| Token | Used for |
|---|---|
| `--chrome` | toolbar, tab bar, sidebar, status bar |
| `--bg` | content: grids, documents, the active tab |
| `--pop` | overlays: palette, peek, popovers, context menus, dialogs; clearly brighter than `--bg` |

Separators between surfaces are `--line` hairlines. No inset panels, nested surfaces or tinted cards; the
inset canvas and its tokens (`--bg-panel`, `--bg-surface`, `--shadow-panel`, `--radius-panel`, `--panel-gap`)
go in U2b.

## Color schemes and tokens (from U2a)

A scheme is a complete token set in `src/styles/schemes/<id>.css`, applied through
`:root[data-scheme="<id>"]`. Settings → Appearance picks one scheme for light and one for dark. Built-in:
`ayu-mirage` (default dark) and `ayu-light` (default light). No color literal outside a scheme file; a
Vitest scan enforces it.

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
| `--scrim` | `rgba(10,12,18,.55)` | `rgba(40,50,70,.2)` | behind modal dialogs |
| `--a-orange` | `#ffa659` | `#fa8d3e` | accent hue (default) |
| `--a-gold` | `#ffcc66` | `#f2ae49` | accent hue |
| `--a-blue` | `#73d0ff` | `#399ee6` | accent hue |
| `--a-purple` | `#d4bfff` | `#a37acc` | accent hue |
| `--a-graphite` | `#8a919e` | `#8a9199` | accent hue |
| `--style-red`, `--style-green`, `--style-yellow`, `--style-blue`, `--style-purple` | per scheme | per scheme | value-style colors |
| `--code-*` | per scheme | per scheme | CodeMirror syntax colors |

Derived tokens in `src/styles/theme.css`, the same for every scheme:

| Token | Value | Role |
|---|---|---|
| `--accent` | `var(--a-<choice>)` via `html[data-accent]` | interactive things only |
| `--o2`, `--o3` | `--text` at 62% and 40% | secondary and tertiary text (see opacity levels) |
| `--cell` | `--accent` at 24% | hovered value |
| `--sel` | `--accent` at 20% | selected row or sidebar entry, focused value |
| `--lit` | `--accent` at 15% | linked table highlight |
| `--pad` | 1, 3, 5 or 8px | row padding (see row anatomy) |
| `--row-h` | `20px + 2 × --pad` | row height for CSS and the virtualizer |
| `--font-ui`, `--font-mono` | Geist, JetBrains Mono | bundled defaults; Settings accepts any installed family |

Status colors (`--success`, `--warning`, `--danger`, `--info` and their `-soft` variants), `--type-*`,
`--chart-*`, `--category-*` and `--tile-*` are removed. Until the components that use them are rewritten
they survive as aliases marked `/* alias: removed in U2b/U12 */`.

## Type (from U2a)

| Token | Size | Used for |
|---|---|---|
| `--fs-small` | 11.5px | column header labels, relation text and counts in section bands, captions |
| `--fs-base` | 13px (sans), 12.5px (mono) | everything else |
| `--fs-large` | 17px | the object header (table name and `#key`) only |

Sizes are in rem, so ⌘= and ⌘- scale the whole UI. Line height is 20px at the base size. Weights: 400, 500,
and 600 for the table name in section bands and the object's `#key`. Numbers use tabular figures and are
right-aligned in grids.

## Opacity levels (from U2a)

| Level | Token | Opacity | Used for |
|---|---|---|---|
| Primary | `--text` | 100% | the value you came for |
| Secondary | `--o2` | 62% | keys in key/value rows, units, currency, relation text, relative times, inactive tabs |
| Tertiary | `--o3` | 40% | column types, column header labels, counts, empty states, `NULL`, `""`, struck old values, key caps |

## Row anatomy (from U2b)

Every list shares one row: document rows, grid rows, sidebar items, palette results, Project page lists.

- **Padding:** `--pad` above and below a 20px line. Settings → Appearance "row padding" has four steps
  (1px, 3px, 5px, 8px); the default is step 2 (3px).
- **Hairline:** every row has a 1px `--line` below it.
- **Row hover:** the whole row takes `--row-hover` (subtle).
- **Cell hover:** the hovered value itself takes `--cell` (stronger), with a 3px radius.
- **No row-number column.** The primary key identifies a row; result sets without one show no number.
- **Key/value rows** (Document): a 300px key column in `--o2`, the value in primary text.
- **Column header row:** column names in `--o3`, small sans; grids show the column type on a second line.
  Table groups always have one, so every value is labelled.
- **Sidebar items:** 8px inset, 5px radius; hover `--row-hover`; the active table `--sel`.

## Section header band (from U7)

Each group in a document starts with a band: `--band` background, `--line` hairlines above and below, 28px
of space above it, 6px 12px inside.

- table name: mono, semibold, primary text (sans for `events` and `changes`);
- relation in secondary small text, mirroring the group's link: `this row`, `meta · order_id`,
  `items · order_id → this row`, `pattern · <rule as SQL>`;
- count right-aligned in tertiary small text (`13 columns`, `9 keys · id order`, `2 rows`);
- capped groups end with `+n keys` / `+n rows`, which open the Rows view; groups with 0 rows collapse into
  one tertiary "Also linked:" line.

The object header is its own block above the groups: table name (`--o2`, large mono), `#key` (accent,
semibold), the Document/Rows segmented control with its `Tab` key cap, the summary as `key value` pairs,
closed by a hairline.

**Linked table highlight (from U7):** hovering a table in the sidebar lights every band of that table
(`--lit` background, 3px accent bar on the left, accent table name); hovering a band lights its sidebar
entry (`--lit` and a 1px accent outline). Hovering a value lights only its provenance table in the sidebar.

## Inputs (from U2b)

Inputs look like inputs, never like rows.

- **Field:** `--field` background, 1px `--field-line` border, 6px radius, a slight inset shadow, a leading
  icon, placeholder in `--o3`, optional key cap on the right. The toolbar search field
  ("Order ID, email, PayPal ID, table…", `⌘K`) opens the palette; the sidebar filter carries `⌘⇧F`.
- **Segmented control:** a field-styled track with 1px padding; segments in `--o2`; the selected segment on
  `--pop` with primary text and a small drop shadow. Used for Document/Rows and for small settings choices.
- **Key cap:** mono 11px, `--o3`, 1px `--field-line` border, 4px radius.

## Overlays (from U2b)

Palette, peek, popovers, context menus and dialogs: `--pop` background, 1px `--field-line` border, 10px
radius, `--shadow` (a glow with zero offset, no 3D lift). Modal dialogs sit over `--scrim`. Overlays take
`Esc` first.

## Value grammar (from U5)

One component (`src/components/value/Value.tsx`) renders every value in grids, documents, peek, the palette,
the readout and the Project page. Hover never opens a tooltip: the status bar shows the provenance of the
hovered value, else of the focused value, else the current address. The Hover column is what the status bar
shows.

| Kind | Look | Click | Hover |
|---|---|---|---|
| PrimaryKey `#3297` | accent, mono | opens the object (Rows view when already in its Document) | provenance |
| Reference, declared `customer_id 4` | accent, dotted underline | opens the target in a new column | provenance and source (FK, project, preset); ⌥ peeks |
| Reference, inferred | accent, dashed underline, tertiary "inferred" | as declared; several targets: choose | the rule (`action_id = wp_actionscheduler_actions.action_id · indexed`) |
| Reference, dangling | tertiary, struck through | none | "no row `#4` in `wp_users`" |
| No reference (`0` or `NULL` in a reference column) | tertiary, no underline | none | "0 = no reference" |
| Enum (status slugs, gateways) `wc-failed` | raw string, default color | filters rows by this value | provenance |
| Number | tabular figures, right-aligned in grids | none | provenance |
| Money | amount primary, currency secondary (`95.40 EUR`) | none | both sources |
| Timestamp | raw value primary, relative time secondary (`1792167625  2 h ago`) | none | UTC and local time |
| Null | italic tertiary `NULL` | none | provenance |
| Empty | tertiary `""` (never rendered like `NULL`) | none | provenance |
| Serialized | tertiary preview `▸ a:17{…}` / `{…17}`; in grids followed by the raw text, tertiary | expands inline as a tree | provenance |
| Secret | masked | reveals | provenance |
| Binary | tertiary `<n bytes>` | none | provenance |

- **Trees** (`ValueTree`): a hairline guide on the left, keys secondary, container types tertiary
  (`array(2)`), strings in dim quotes so `"2.04"` and `2.04` differ, leaf pairs of a small array on one line.
  Tree rows keep the row anatomy.
- **Value styles (from U4c):** a project (or preset, off by default) rule `{ columns or meta keys, match:
  = | LIKE | REGEXP, value, color }` colors matching values with `--style-<color>` (`red`, `green`, `yellow`,
  `blue`, `purple`). A style is always scoped to columns or meta keys, never global. There is no status kind
  and no default status color.
- **Provenance text:** `wp_wc_orders_meta.meta_value · id 9123`, plus `→ wp_users.ID · preset woocommerce`
  for references and `UTC … · local …` for timestamps.
- **Change marks (from U13):** a changed value flashes (`--cell` fading out over 1.6s), then shows the old
  value tertiary and struck through, with a 3px accent bar on the row.

## Layout

- **Toolbar** (`--chrome`): connection and database as plain mono text, the path of the column stack
  (`order #4711 › user #12 › payment_token #7`, from U9), the search field on the right (from U2b).
- **Tab bar** (from U2b): tabs are cells separated by `--line`; the active tab takes `--bg` and joins the
  content; preview tabs show italic titles. Tabs: query, table, object (from U7), project (from U8).
- **Sidebar:** the table tree with its filter; toggled with `view.sidebar` (from U2b), hidden entirely when
  collapsed, persisted per connection.
- **Status bar** (from U2b), left to right: ▤ sidebar toggle, provenance or address, spacer, `pending n`,
  `● live 2 s` in the accent while live (from U13). Plain text, no chips.
- **Column stack** (from U9): drilling opens a column to the right (minimum 420px); the oldest collapse to
  34px strips with a vertical `table #key` label; the active column has a 2px accent line on top.

## Keys

Defaults; every key is a rebindable command (Settings → Shortcuts, `keybindings.json`). Bare keys fire only in
the object and grid scopes, never in inputs, the editor or dialogs; `Tab` keeps its normal meaning there.

| Key | Scope | Action |
|---|---|---|
| `Tab` | object | Document ↔ Rows for the focused column; focus stays on the same provenance (from U7) |
| `j` / `k`, ↓ / ↑ | object, grid | move between values / rows (from U9) |
| `h` / `l`, ← / → | object | move between columns (from U9) |
| `Enter` | object, grid | drill into the focused reference / open the row as an object (from U9) |
| `Esc` | object | close the rightmost column; overlays take `Esc` first (from U9) |
| ⌘[ / ⌘] | global | back / forward through address history (today WordPress only; every tab from U3) |
| `⌥` (held) | object, grid | peek the focused or hovered reference (from U9) |
| `y` / `Y` / `e` / `f` | object, grid | copy raw / copy as SQL condition / edit / filter by value (from U5) |
| `r` | object, grid | add a reference from this column or meta key (from U8) |
| `w` | object | live mode on/off (from U13) |
| `s` | object | snapshot (from U13) |
| `x` | object | dismiss change marks in the focused column (from U13) |
| `p` | query, table | pin the current query to the readout (from U11) |
| ⌘\ | global | toggle the sidebar (from U2b) |
| ⌘⇧F | db | focus the sidebar filter |
| ⌘K | global | palette |
| `?` | global | shortcut overview (from U9) |

## Palette prefixes

The palette is the main navigation. Results render with the value grammar (from U10). Enter opens in the
current tab (the object tab's column stack from U9); ⌘↵ opens a new tab.

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
