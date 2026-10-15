# Roadmap

Ideas collected during the MVP. Not ordered by priority.

## Core client
- SQLite driver: a second `Driver` variant; the SQL lexer and value model already apply.
- Export results (CSV/JSON/SQL), and import.
- SSH tunnels (staging databases), e.g. via `russh`.
- Stream large results over `tauri::ipc::Channel` instead of one JSON payload.
- Generate TS types from Rust DTOs (ts-rs or tauri-specta).
- Multiple windows; resizable grid columns; query plan (EXPLAIN) view.

## WordPress lens
- Taxonomies resolved by name (`product_cat: [Mugs, Sale]`).
- ACF labels from `field_xxx` keys and `acf-field` posts.
- HPOS vs. legacy diff when sync is on: `wc_orders` and the `shop_order` post side by side, with mismatches highlighted.
- Check `wc_product_meta_lookup` against product meta.
- Entity query language that compiles to SQL and shows it, e.g. `order where status=processing and meta._payment_method=ppcp-gateway and created > -7d`.
- Integrity scanner: orphaned meta, dangling term relationships, items pointing to deleted products, overdue actions, HPOS sync drift.
- Multisite: switch the lens between sub-site prefixes.

## Done (WordPress mode)
Dashboard, list screens, purpose-built pages, order timeline, ⌘K search, edits with a pending tray and undo, serialized-value tree editing, live mode, snapshots and diffs, side-by-side comparison, options treemap, screen-share mode.
