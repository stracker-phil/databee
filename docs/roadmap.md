# Roadmap

Ideas collected during the MVP. Not ordered by priority.

## Core client
- SQLite driver: a second `Driver` variant; the SQL lexer and value model already apply.
- Export results (CSV/JSON/SQL), and import.
- SSH tunnels (staging databases), e.g. via `russh`.
- Stream large results over `tauri::ipc::Channel` instead of one JSON payload.
- Generate TS types from Rust DTOs (ts-rs or tauri-specta).
- Multiple windows; query plan (EXPLAIN) view.

## WordPress lens
- Taxonomies resolved by name (`product_cat: [Mugs, Sale]`).
- ACF labels from `field_xxx` keys and `acf-field` posts.
- HPOS vs. legacy diff when sync is on: `wc_orders` and the `shop_order` post side by side, with mismatches highlighted.
- Check `wc_product_meta_lookup` against product meta.
- Entity query language that compiles to SQL and shows it, e.g. `order where status=processing and meta._payment_method=ppcp-gateway and created > -7d`.
- Integrity scanner: orphaned meta, dangling term relationships, items pointing to deleted products, overdue actions, HPOS sync drift.
- Multisite: switch the lens between sub-site prefixes.

## Plugin lens
- PHP code scanning (tree-sitter) of installed plugins to learn their option/meta keys exactly.
- Runtime attribution via a mu-plugin that records which plugin writes which key.
- Attribute Action Scheduler groups and log sources that use generic names.
- Rebuild the native menu live when a menu-bound shortcut changes (today: after restart).
- Split view for entity pages opened from the palette (open in a new tab).

## Done (WordPress mode)
Dashboard with actionable alerts, list screens with split view and sorting, purpose-built pages, humanized order
timeline, global ⌘K palette with a command registry and rebindable shortcuts, peek cards, value pivots and
find-everywhere, guest customers, plugins lens with leftover cleanup, edits with a pending tray and undo,
serialized-value tree editing, live mode, snapshots and diffs, side-by-side comparison, screen-share mode.
