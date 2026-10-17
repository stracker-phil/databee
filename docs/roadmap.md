# Roadmap

Ideas not built yet. Not ordered by priority. Shipped features are in the [README](../README.md).

## Version 2
- **Local graph.** `graph(address, depth: 1 | 2) -> Graph { nodes, edges }`. Center: the object; ring 1:
  references, incoming reference groups, pattern groups; ring 2 toggled with `2`. Declared edges solid,
  inferred ones dashed, labelled with the linking column and condition; references with many rows
  aggregate into one node above a cap. SVG with a deterministic radial layout, so positions stay stable
  while you drill; `g` opens it, `Enter` navigates and re-centers. Address `?view=graph`.
- **Schema graph.** Tables and their references (foreign keys, project, preset, inferred), for a plugin's
  tables you haven't seen before. Address `?view=schema`. Add missing references from the graph, as on
  the Project page.
- **Value-level inference.** A meta value or decoded action argument that equals an existing id (integers
  greater than 1, not covered by a money, time, flag or count kind, looked up in the entities' tables with
  one batched query each). Marked inferred with its rule, capped per document; grids infer only on peek;
  "promote to reference" turns a hit into a project reference with a condition. Needs its own rule in
  [`.claude/rules/presets.md`](../.claude/rules/presets.md).

## Project page
- Inferred references collapsed into one group per table with "enable all" / "disable all".
- Actions listed per table with what they write (the tables and columns of `update`, `delete`, free SQL).
- Kinds folded into a details disclosure per table.
- Filter by origin (project, preset, inferred, foreign key, edited).

## Core client
- SQLite driver: a second `Driver` variant; the SQL lexer and value model already apply.
- Export results (CSV/JSON/SQL), and import.
- SSH tunnels (staging databases), e.g. via `russh`.
- Stream large results over `tauri::ipc::Channel` instead of one JSON payload.
- Multiple windows; query plan (EXPLAIN) view.

## Presets
- Taxonomies resolved by name (`product_cat: [Mugs, Sale]`).
- ACF labels from `field_xxx` keys and `acf-field` posts.
- HPOS vs. legacy diff when sync is on: `wc_orders` and the `shop_order` post side by side, with
  mismatches marked (today: the `hpos_drift` anomaly counts them).
- Check `wc_product_meta_lookup` against product meta.
- Entity query language that compiles to SQL and shows it, e.g. `order where status=processing and
  meta._payment_method=ppcp-gateway and created > -7d`.
- More integrity anomalies: orphaned meta, dangling term relationships, items pointing to deleted
  products.

## Plugin attribution
The prefix-based plugins lens was removed: naming heuristics can't tell which plugin owns a key
(`woocommerce-ppcp-*` belongs to PayPal Payments, not WooCommerce). Only exact sources qualify:
- PHP code scanning (tree-sitter) of installed plugins to learn their option/meta keys exactly.
- Runtime attribution via a mu-plugin that records which plugin writes which key.
