# Roadmap

Ideas collected during the MVP. Not ordered by priority.

## Core client
- SQLite driver: a second `Driver` variant; the SQL lexer and value model already apply.
- Cell editing and inserts, with an explicit edit mode and a preview of the diff before writing.
- Export results (CSV/JSON/SQL), and import.
- SSH tunnels (staging databases), e.g. via `russh`.
- Stream large results over `tauri::ipc::Channel` instead of one JSON payload.
- Generate TS types from Rust DTOs (ts-rs or tauri-specta).
- Multiple windows; resizable grid columns; query plan (EXPLAIN) view.

## WordPress lens
- Taxonomies resolved by name (`product_cat: [Mugs, Sale]`).
- ACF labels from `field_xxx` keys and `acf-field` posts.
- HPOS vs. legacy diff when sync is on: `wc_orders` and the `shop_order` post side by side, with mismatches highlighted.
- Order timeline: notes, Action Scheduler actions and logs, status changes and webhooks in one chronological list.
- Product tree: parent, variations and attribute matrix; check `wc_product_meta_lookup` against the meta.
- Sessions (`woocommerce_sessions`) shown as a readable cart with an expiry countdown.
- Payment tokens linked to the user and gateway (entity exists; needs a dedicated view).
- Snapshot and diff of an entity graph (stored in local SQLite, as path-level diffs). For example: watch an order, its customer, `woocommerce_ppcp%` options and Action Scheduler, place a test checkout, then diff.
- Entity query language that compiles to SQL and shows it, e.g. `order where status=processing and meta._payment_method=ppcp-gateway and created > -7d`.
- Integrity scanner: orphaned meta, dangling term relationships, items pointing to deleted products, overdue actions, HPOS sync drift.
- Safe write-back of serialized values edited as a tree. The serializer already round-trips exactly; warn about object-cache and HPOS-sync bypass.
- Multisite: switch the lens between sub-site prefixes.
