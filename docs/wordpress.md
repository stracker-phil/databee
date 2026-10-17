# WordPress and WooCommerce presets

WordPress knowledge is configuration: three built-in presets in `src-tauri/presets/`, written in the format of
[presets.md](presets.md) and governed by [`.claude/rules/presets.md`](../.claude/rules/presets.md). The engine
(`src-tauri/src/rules/`) knows no WordPress names. The WordPress screens in `src-tauri/src/wp/` and
`src/components/wp/` still exist and read the presets through small adapters ([Screens](#screens-until-u12));
U8 to U12 replace them with generic views. Since U7 a WordPress record (`order 4711`, `user 12`) opens as an
object tab: the generic document of its row ([frontend.md](frontend.md#object-tabs)); the purpose-built entity
views are gone.

## The presets

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
  `set_status` (on both shapes, each also updating the other table) and `delete_logs_before`; find sources for
  log messages and order ids.

**`action-scheduler.toml`**
- References `group_id`, `claim_id` and `logs`; entity `action`; enums `status` and `hook`; timestamps
  (`log_date_gmt` UTC, `*_local` local; the other `*_gmt` columns are UTC through `wordpress`); value styles
  for `failed` and `complete`, shipped off.
- Pinned queries `actions`, `actions_failed_by_hook`, `actions_overdue`; anomalies `actions_failed`,
  `actions_overdue`; actions `retry` and `cancel`; find source `args`/`extended_args`.

To change a preset for one machine, copy it to `<config dir>/presets/` and edit it: a user preset with the same
`name` replaces the built-in ([presets.md](presets.md#where-presets-live)). The old
`<config dir>/entities.toml` is no longer read: when it exists, the log warns once at startup and the page shows
a toast once per run (`WpInfo.legacyEntitiesFile`); nothing is converted.

## Screens (until U12)

The backend is an inline Tauri plugin in `src-tauri/src/wp/`, installed by `.plugin(wp::init(&config_dir))` in
`lib.rs`, with its commands in `wp::COMMANDS` and `WP_COMMANDS` in `build.rs` (granted by
`capabilities/wp.json`). Its state `WpState` holds the snapshot store and the legacy `entities.toml` path.

Every command builds a `WpEnv` from the database's effective rules (`commands::load`): the schema, the cached
`EffectiveRules`, a `site::WpContext` and a `catalog::Catalog`. A database is WordPress when the `wordpress`
preset applies.

| File | Responsibility |
|---|---|
| `site.rs` | `WpContext`: the prefix, `table(name)` (network tables under the network prefix), `has_table`, `woocommerce()` (the preset applies) and `hpos()` (orders are read from `wc_orders` wherever that table exists) |
| `catalog.rs` | `Catalog` from the rules: one `EntityDef` per entity name (an unfiltered entity wins, so `order` is `wc_orders` where it exists and `posts` otherwise), its owned meta table, `links` (unconditional references to another entity's key), `meta_links` (references from the meta value by meta key), `relations` (labelled references to its key); `table_entities`; the document SQL |
| `document.rs` | `EntityDoc` and `DocField`; `assemble` adds a `Hint` (money with the currency of the row or the document, time for `unix`/`utc` timestamps) and `Sensitive::Secret` from the kind rules; the title is the entity's summary values |
| `mod.rs` | `WpInfo` (the `rules_info` shim), `load_docs` and `expand` (only for snapshots: rows, meta and relation counts), featured images (`post_images`, for the lists), options queries |
| `commands.rs` | The Tauri commands, which only forward; `wp_detect` maps `rules_info` to `WpInfo` |
| `lists.rs`, `dashboard.rs`, `options.rs`, `snapshot.rs`, `changes.rs` | The list screens, dashboard (UTC days), options inspector, snapshots and writes; the palette's search is the core's `search/` ([architecture](architecture.md#palette-search)) |
| `sql.rs`, `rows.rs`, `media.rs`, `snapshots_store.rs` | Shared SQL fragments, cell readers (`money`: an amount as stored plus its currency code), thumbnail URLs, the snapshot store |

`wp_detect` returns `WpInfo { prefix, presets, entities, tableEntities, legacyEntitiesFile }` or `null`.
`entities` are the entity names of `RulesInfo.entities`;
`tableEntities` maps a real table to the entity its rows open as. `openEntity(entity, id)` (`wp` slice) asks
`document` for the shorthand `<entity> <id>`, keeps the reply (`keepDocument`) and opens its canonical row address
as an object tab. A list's side pane shows the same document (`object/EntityDocument.tsx`). The Compare screen
and the entity page's snapshot button are gone until U13; live mode is absent until then. There is no site URL, WooCommerce version,
HPOS mode, time zone or store currency any more: the status bar shows the prefix, amounts show as stored with
their own currency, and the dashboard counts days in UTC.

The remaining screens' writes are `WpChangeRequest` variants planned by `wp_plan_change` (`changes::plan`) into
the session's plans: `SetAutoload` and `DeleteExpiredTransients` (Options screen) and `DeleteLogsBefore` (dashboard
insights). Meta edits, order status, retry and cancel moved to core edits on the document's rows (`setCell` on
the provenance row) and to the presets' rule actions (`set_status`, `retry`, `cancel`, `plan_action`). Raw SQL
writes run no WordPress hooks. Undo restores old values exactly
(`db::changes::restore_literal`); literals follow the server's `sql_mode` (`Dialect::sql_string`).

## Tests

- Preset tests: the built-in files parse, round-trip and apply on the fixtures (`src-tauri/tests/rules.rs`,
  against `wp_test`, `wp_legacy`, `wp_custom` with prefix `xy_`, `wp_multi` and `databee_test`).
- `src-tauri/src/wp/server_tests.rs` holds the `#[ignore]` integration tests of the remaining screens; they run
  with `bun run test:db`.
- Frontend: `src/components/wp/WpMode.test.tsx` plus tests per component.
