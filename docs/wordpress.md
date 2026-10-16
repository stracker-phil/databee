# WordPress layer

WordPress mode is a lens over a WordPress (and WooCommerce) database. It is built for developers who inspect
their own local sites. It complements wp-admin and the code; it doesn't replace them. The rules in
[`.claude/rules/wp-layer.md`](../.claude/rules/wp-layer.md) govern every screen:

- **Real names.** Show the stored names: columns, meta keys, option names, status slugs. A human label may sit
  next to a raw value, never in place of it.
- **No guessing.** Show only facts the database proves. There is no attribution by naming heuristics. Meta and
  options are grouped by their literal key prefix (`_ppcp*`), never by "which plugin owns this".
- **Nothing hidden.** Never hide stored data. Collapsing it is the most that's allowed.
- **Traceable.** Every derived value names its source key. Every entity can be opened as its rows in the
  Database view.
- **Secrets masked, nothing else.** There is no PII masking. Secrets such as keys and tokens stay masked until
  clicked.

The backend lives in `src-tauri/src/wp/` and the commands in `src-tauri/src/wp_commands.rs`. The frontend lives
in `src/components/wp/`.

## Modules

| File | Responsibility |
|---|---|
| `mod.rs` | Detection I/O, document loading, link resolution, options queries, `fetch` (30 s timeout) |
| `context.rs` | Detection logic with no I/O: prefix, versions, WooCommerce, HPOS mode, multisite, site facts |
| `php.rs` | Lossless parser and serializer for PHP `serialize()` data |
| `value.rs` | Turns stored text (PHP or JSON) into a generic `ValueNode` tree. Serialized strings inside other values are decoded up to 3 levels deep |
| `entity.rs` + `entities.toml` | Entity definitions as data, plus the resolver that turns them into SQL and documents |
| `hints.rs` + `meta_keys.toml` | Render hints (money, time, flag, URL, user agent, email, roles, country, guest) and secret masking, keyed by glob rules |
| `lists.rs` | List screens: orders, subscriptions, users (including guests), products, actions, sessions, content. One query per page, plus one batched query for extras and facets |
| `detail.rs` | Purpose-built views (order, customer, guest, product, action, session), the order timeline, and the ⌘K omnisearch |
| `dashboard.rs` | Dashboard metrics, insights, sidebar counts, action histogram |
| `options.rs` | Options inspector: autoload by prefix, transients, cleanup SQL |
| `peek.rs` | Hover peek facts and "find everywhere" |
| `snapshot.rs` | Snapshots, diffs and comparisons over flattened `path → value` maps |
| `changes.rs` | Every write: planned SQL plus undo SQL, and edits to serialized values |
| `logs.rs` | Short titles for WooCommerce log lines, such as `PayPal API · capture order → 201` |
| `media.rs` | Thumbnail URLs from attachment meta |

## Detection

`wp::detect`, then `context::build`:

1. **Prefix.** A prefix qualifies when `{p}options`, `{p}posts` and `{p}users` all exist; the shortest one wins.
   Multisite sub-sites have no users table, so they never win. Custom prefixes work.
2. **Options.** One query reads the `PROBED_OPTIONS`: versions, HPOS flags, `siteurl`, `blogname`, currency,
   price decimals (`woocommerce_price_num_decimals`, default 2), time zone, theme, `active_plugins`, uploads path.
3. **Facts:**
   - **WooCommerce:** a `woocommerce_version` option, or the WooCommerce order tables.
   - **HPOS:** `On` only when `woocommerce_custom_orders_table_enabled = yes` *and* `wc_orders` exists.
     `OnWithSync` when data sync is also on.
   - **Legacy store:** WooCommerce with HPOS off keeps orders in `posts` (`shop_order`, customer in the
     `_customer_user` meta). Entities and relations carry a `when` (`hpos`, `legacy_orders`), and both the
     catalog and `wp::load_docs` skip those that don't apply, so the `user.orders` relation (an
     `wc_orders.customer_id` column) is not counted on legacy stores. Customer views list legacy orders through
     `_customer_user` instead.
   - **Action Scheduler:** whether its tables exist.
   - **Multisite:** sites come from `{main}{N}_options` plus `wp_blogs`.
   - **Time zone:** `SiteMeta.zone` is a `context::SiteZone`: `Named` from `timezone_string` (with DST, via
     `chrono-tz`), or `Fixed` from `gmt_offset` (hours, may be fractional) when `timezone_string` is empty or
     not a zone name. `SiteMeta.zone_source` names the option it came from. Local `DATETIME` values without
     `gmt` in the key become instants through `SiteZone::to_utc` (`hints::time`). The dashboard and the action
     histogram bucket by local day (and hour): `dashboard::day_spans` / `hour_spans` turn the site's days into
     UTC ranges for a `CASE` over the `*_gmt` column, and "today" comes from the server's `UTC_TIMESTAMP()`.

The result `WpContext` is cached per database in the session's `WpCache` (`wp/cache.rs`, attached to
`Session.ext`), until `wp_detect(refresh)` or a successful `apply_changes` (which clears the session's cache,
since statements name no database). The cache dies with the session, so a reconnect detects again.
Only WordPress databases are cached, so one that becomes WordPress is detected on the next call.
The UI receives `WpInfo`, which holds the context, the entity names for the palette, and a map from table to
entity. That map lets the Database view offer "Open as WordPress document".

Multisite sub-sites are detected but not used yet. All queries go to the main prefix.

## PHP serialize

`php.rs` round-trips exactly: `serialize(parse(x)) == x` for valid input. The test suite checks this, and
`tests/php_roundtrip.rs` checks it against every serialized value of a real database. The parser:

- keeps **strings as bytes**, because `s:N` counts bytes; invalid UTF-8 is preserved;
- keeps **floats as their source text**;
- keeps **entry order**;
- **never runs or instantiates anything:** `O:` objects become a class name plus properties, while `C:`,
  references and PHP 8.1 enums stay as opaque nodes;
- applies **limits:** nesting depth 128, preallocation capped at 1024; garbage input returns `ParseError`.

Decoding modes:
- **Strict** parsing fails at the first wrong byte length.
- **Lenient** parsing ignores `s:N` and finds the end of each string by its delimiter. This repairs data that
  was re-encoded after it was serialized.
- `value::decode` tries strict first. If only lenient succeeds, it returns the tree with a warning.

**Editing** (`changes::edit_serialized`):

1. The UI sends the raw value, a path of tree keys and a typed `NewValue`.
2. The path may step into a serialized string inside another value (segment `"decoded"`).
3. The whole value is re-serialized, which recomputes every `s:N`. Untouched parts, including whitespace
   around the value, stay byte-identical.
4. The result is only a string. It is written through the usual `queueChange` pipeline.

Only values that parse strictly can be edited. Refused edits:
- a float that is not PHP float syntax (digits with optional sign, fraction and exponent, or `INF`, `-INF`,
  `NAN`); Rust spellings like `inf` or `infinity` would be unreadable for `unserialize()`;
- any edit when an `r:`/`R:` reference sits at or after the edited value: references count values in
  serialization order, so replacing a subtree could make them point elsewhere.

## Entities

An entity is a table described as data. Most of WordPress mode works for any entity without Rust code: generic
document views, links, relations, the Database-view row action, the palette, snapshots, compare, and meta
edits.

- **Built-in definitions:** `src-tauri/src/wp/entities.toml`, compiled in. They cover post, user, comment,
  order (HPOS and legacy), order item, order address, order operational data, payment token, action and
  session.
- **User definitions:** `<config dir>/entities.toml`, re-read on every command. A user definition replaces a
  built-in one when **both** `name` and `when` match; otherwise it is added.
- **Selection:** the catalog keeps definitions whose `when` applies and whose table exists. The first match
  per name wins. That is how `order` resolves to `wc_orders` under HPOS and to `posts` on legacy stores.

| Key | Meaning |
|---|---|
| `name`, `label` | id used in routes and links; display label |
| `when` | `always` (default), `woocommerce`, `hpos`, `legacy_orders` |
| `table`, `id` | table **without** prefix; unsigned integer id column |
| `title`, `subtitle` | columns for the document heading |
| `filter` | raw SQL added as `AND (filter)`; trusted config |
| `hidden` | columns left out of the document (e.g. `user_pass`) |
| `meta` | `{ table, fk, key = "meta_key", value = "meta_value" }` |
| `links`, `meta_links` | `{ column_or_meta_key = "entity" }`: values become links |
| `relations` | `[{ name, label, entity, fk, filter?, when? }]`: one-to-many, counted in one batched query |

```toml
[[entity]]
name = "vault_token"
label = "PayPal vault token"
when = "woocommerce"
table = "ppcp_vault"          # becomes {prefix}ppcp_vault
id = "id"
title = "vault_id"
subtitle = "status"
meta = { table = "ppcp_vaultmeta", fk = "vault_id" }
links = { user_id = "user", order_id = "order" }
```

**Resolving.** The resolver generates `SELECT * … WHERE id IN (…)`, one meta query and one count query per
relation. Ids must parse as `u64`, so they are safe to inline. `assemble` builds `DocField`s, each holding:

- the raw `Cell`;
- a decoded tree;
- a link;
- a render hint;
- a sensitivity flag.

It also groups meta by literal key prefix and records the source table and id column, so every field can be
opened as its row.

## Screens

Routes are a `WpRoute` union in `src/state/store.ts`: dashboard, list, entity, compare, snapshots and find.
Navigation keeps a browser-like history. `useWpData(key, load)` loads data and reloads it whenever the
session, database, `key` tuple or the store's `dataVersion` change; it never returns another database's data.
`dataVersion` is bumped after every apply.

List filters (status, kind, search, sort, order, offset) are part of the route: `Lists.tsx`
`useListFilters` writes them to `route.params` with `replaceRoute`, and any change except paging resets the
offset in the same update, so back and forward restore them. Arrow keys move the selection only while the
split detail pane is shown. The content list without a kind lists every visible post type. Peek facts
(`Peek.tsx`) are cached per session, database, entity and id for one `dataVersion`.

| Screen | Backend | Component |
|---|---|---|
| Sidebar with counts and attention badges | `wp_browser` → `dashboard::browser` | `WpWorkspace.tsx` |
| Dashboard: tiles, insights, autoload breakdown, order bars, Action Scheduler, recent orders | `wp_dashboard`, `wp_options` | `Dashboard.tsx`, `BreakdownChart.tsx`, `charts.tsx` |
| Lists (50 per page, facets, search, sort from a whitelist, split pane) | `wp_orders`, `wp_users`, `wp_products`, `wp_actions`, `wp_sessions`, `wp_content` | `Lists.tsx` |
| Options: autoload weight, largest options, groups by prefix, transients | `wp_options`, `wp_options_group` | `Settings.tsx` |
| Entity pages: order, customer/guest, product, action, session, generic | `wp_view` → `detail::entity_view`, `wp_order_logs` | `views/EntityScreen.tsx`, `OrderView.tsx`, `OtherViews.tsx`, `common.tsx` |
| Field values: hints, masked secrets, links with hover peek, editable trees, context menu | `wp_peek`, `edit_serialized` | `FieldValue.tsx`, `Peek.tsx`, `LinkChip.tsx`, `../ValueTree.tsx` |
| Find everywhere | `wp_find_value` | `FindScreen.tsx` |
| Snapshots and diff; compare two entities | `wp_snapshot_*`, `wp_compare` | `Debugging.tsx` |
| ⌘K entity search | `wp_search` | `../CommandPalette.tsx` |

Every purpose-built view also embeds the generic document, so no stored field is hidden. Entity pages have
Live mode (polls every 3 s and highlights changes), Snapshot, Compare and Source row.

### Order logs

`detail::order_logs` (`wp_order_logs`) lists `woocommerce_log` rows from a day before the order's creation to
90 days after it. `detail::order_log_patterns` builds the match: `message REGEXP '#<id>([^0-9]|$)'` (so `#4711`
does not find `#47110`), the context's `order` key in the serialized form `WC_Log_Handler_DB` stores
(`s:5:"order";i:<id>;` or the string variant) and in JSON (`"order": ?<id>[^0-9]` as a REGEXP), and the
PayPal order and transaction IDs anywhere in the message. Every LIKE value goes through `lists::like`, which
escapes `%`, `_` and `\`. `detail::log_level` maps the `WC_Log_Levels` scale: up to 300 (debug, info, notice)
is info, 301 to 400 (warning) is warning, above 400 is error.

Action pages (`detail::action_view`) decode `extended_args` when it is set, because Action Scheduler then
stores only a hash in `args`.

### Peek

`peek::peek` returns up to seven facts from `peek.rs` `FACTS`. For users on HPOS stores the order stats
(Orders, Spent) come right after the first identity fact.

### Money

`hints::money(raw, currency, decimals)` parses the stored text as an exact `rust_decimal::Decimal`, rounds half
away from zero to `SiteMeta.price_decimals` and never shows `−0.00`. Text that is not a finite number (`inf`,
`NaN`, words) comes back unchanged without a currency, and money hints are only attached to values that parse.
PayPal fee breakdowns (`detail::paypal_fees`) use `hints::DEFAULT_PRICE_DECIMALS`.

## Writes

Every WordPress write is a `ChangeRequest` variant planned by `changes::plan` and applied through the pipeline
in [database.md](database.md#editing-rows):

| Variant | Effect | Undo |
|---|---|---|
| `SetCell`, `DeleteRow` | key must match one row | yes; none for binary values |
| `DuplicateRow`, `InsertRow` | | none |
| `SetMeta` | inserts, or updates the one row of that key; refuses when the key has several rows | yes |
| `DeleteMeta` | meta rows of an entity; refuses when the key has no row | reinserts exact rows |
| `SetOrderStatus` | `wc_orders.status` plus the posts copy when it exists | yes |
| `SetAutoload` | keeps the stored style: `yes`/`no` or `on`/`off` (WP 6.6+) | yes |
| `DeleteExpiredTransients` | value plus timeout rows, site transients too | reinserts exact rows |
| `DeleteLogsBefore` | WooCommerce log retention, cutoff fixed at plan time; refuses if nothing matches | none |
| `RetryAction`, `CancelAction` | Action Scheduler status | yes |

The tray warns that these are raw SQL writes, so no WordPress hooks run.

Undo fidelity: old values are restored exactly (`changes::restore_literal`). `NULL` stays `NULL` and `''`
stays `''`. Binary cells only carry a preview, so their edits get no undo and the description says why
(`(no undo: …)`). When `SetCell` changes the key column itself, the undo finds the row by its new key.
`DeleteLogsBefore` reads `UTC_TIMESTAMP()` once while planning and embeds the cutoff as a literal, so the
DELETE removes exactly the counted entries.

Escaping follows the server's `sql_mode`: the session reads it when connecting (and when the editor
reconnects) and exposes it as `Database::dialect`. Every literal in `wp/` goes through
`Dialect::sql_string`; under `NO_BACKSLASH_ESCAPES` only quotes are doubled, so a written `\` is stored as one
backslash. LIKE patterns go through `Dialect::like` / `like_contains` (with `db::escape_like`), which adds
`ESCAPE '\'` in that mode because the server then has no default LIKE escape character.

## Extending

**A new entity** needs no Rust:
1. Add an `[[entity]]`, either built-in or in the user file.
2. Add `meta_keys.toml` rules for hints and secrets.
3. Optionally:
   - add peek facts in `peek.rs` `FACTS`;
   - add a find-everywhere source in `peek.rs` `SOURCES`;
   - add omnisearch queries in `detail.rs` `search`.
4. Test it in the `entity.rs` tests.

**A new list screen:**
1. In `lists.rs`, add a summary struct and an `async fn` with a SQL builder that has no I/O and can be unit
   tested. Give it a `SORT_KEYS` entry. It returns `Page<T>`.
2. Expose it: a command in `wp_commands.rs` (use `wp_env`), registration in `lib.rs`, a wrapper and types in
   `src/api/`, and a mock in `src/test/apiMock.ts`.
3. In the frontend:
   - add the list to the `WpList` union in `store.ts`;
   - add `LOADERS`, `COLUMNS`, `cellsFor` and `rowTarget` entries in `Lists.tsx`;
   - add `ICONS`, `NAV_GROUPS` and `ATTENTION` entries in `WpWorkspace.tsx`;
   - add a `BrowserSection` in `dashboard::browser`;
   - add a `go(...)` palette command in `src/commands/index.ts`.

**A new detail view:**
1. In `detail.rs`, add a struct, a loader and an `EntityView` variant, and dispatch it in `entity_view`.
2. Embed the generic `EntityDoc`.
3. Add a component in `views/` and a branch in `EntityScreen.tsx`.

**A new write:**
1. Add a `ChangeRequest` variant that carries `database`.
2. Plan both `statements` and `undo` in `changes::plan`.
3. Add it to the `database` match in `plan_change`.
4. Mirror the type in TypeScript.
5. Trigger it with `queueChange`.
6. Optionally return it from `dashboard::insights` as a one-click action.

**Another plugin** (Subscriptions, Yoast, a payment gateway):
1. Add entities with a fitting `when`. If it needs a plugin-specific condition, add a `When` variant and the
   `WpContext` fact behind it. `site.active_plugins` already holds the plugin slugs, and probed options go
   into `PROBED_OPTIONS`.
2. Missing plugin tables are skipped automatically.
3. Add key rules in `meta_keys.toml`, log humanizers in `logs.rs`, and dashboard insights as needed.
4. Never attribute keys to the plugin by their names.

## Tests

- **Unit tests** in each module cover detection variants, PHP round trips and limits, exact SQL from the
  resolver, serialized edits, insights, SQL builders and log humanizers.
- **`src-tauri/tests/wp.rs`** runs against the Docker fixture `wp_test`: WordPress plus WooCommerce with HPOS and
  sync. Reads go against `wp_test`, and every write goes into its copy `wp_write`, so read and write tests
  never race. `wp_legacy` covers a posts-based (HPOS off) store.
- **`src-tauri/tests/php_roundtrip.rs`** checks round trips against a real database of your own. It only reads:
  `DATABEE_WP_TEST=mysql://user:pass@host:port/db`.
- **Frontend:** `src/components/wp/WpMode.test.tsx` plus tests per component.
