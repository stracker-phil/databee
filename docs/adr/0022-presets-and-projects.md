# 0022. Presets and projects instead of WordPress code

## Status

Accepted (2026-10-17). Supersedes [0008](0008-wp-layer-shows-stored-data.md). Being implemented in U4a to
U4d and U8; `wp/` is removed in U12.

## Context

The WordPress lens (`src-tauri/src/wp/`, `src/components/wp/`) encoded WordPress and WooCommerce knowledge
as Rust and React code next to two TOML files: entity views, peek facts, log matching, lists, dashboard
insights, the options screen and the writes. Each new view meant new code, nothing of it helped a
non-WordPress database, and the rules of [0008](0008-wp-layer-shows-stored-data.md) applied to one layer only.

## Decision

WordPress is a preset. A generic rule engine (`src-tauri/src/rules/`) interprets:

- **Presets**: read-only TOML bundles of rules (meta tables, references, entities, kinds, value styles,
  patterns, pinned queries, anomalies, actions). Built-ins are compiled in (`src-tauri/presets/*.toml`);
  user presets in `<config dir>/presets/*.toml` replace a built-in of the same name. A preset applies when
  its marker tables exist under the project's prefix; there is no version or storage detection.
- **Projects**: per connection slug and database, stored as JSON (`projects.json`, edited in the UI): the
  table prefix, the enabled presets, and own rules of the same shapes that add to or override preset rules.
  A project exports as a preset file.

The presets are the acceptance test: what a WordPress or WooCommerce view needs and the rules cannot
express becomes a generic engine feature, never WordPress code. Every preset and project rule follows
[`.claude/rules/presets.md`](../../.claude/rules/presets.md). Keeping WordPress as a code lens was rejected
because it never generalizes; JSON for presets was rejected because hand-written rule files need comments.

## Consequences

- Config size measures whether the engine works; it is not a risk to minimize.
- Some WordPress conveniences are dropped (version and HPOS detection, site time zone and currency options,
  humanized log titles, computed blocks, charts).
- Rule errors must name the file, the rule and the field, since users write presets by hand.
- A `Lens` trait stays as the seam between commands and rule sources, so a code lens remains possible if
  configuration hits a real wall.
