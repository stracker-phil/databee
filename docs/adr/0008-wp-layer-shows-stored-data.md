# 0008. The WordPress layer shows stored data, no naming heuristics

## Status

Superseded by [0022](0022-presets-and-projects.md) (2026-10-17). Accepted retroactively, introduced in 0d46c2f,
2026-10-16.

WordPress becomes a preset, so these rules no longer belong to one layer. They hold for every preset and
project rule and now live in [`.claude/rules/presets.md`](../../.claude/rules/presets.md), which replaces
`.claude/rules/wp-layer.md` and adds rules for inferred references, value styles and virtual references.

## Context

The audience is senior WordPress developers inspecting their own local databases. Guessing which plugin owns
a meta key or relabelling stored values misleads exactly the people who need the raw facts.

## Decision

The WordPress layer follows `.claude/rules/wp-layer.md` (now `presets.md`): real names
next to any label, only facts the database proves, nothing hidden for looking internal (collapsed at most),
every derived value names its source key and can be opened as rows, and secrets stay masked until clicked.

## Consequences

- The UI is less "friendly" than an admin screen.
- Every view must carry its source keys and a way to the rows.
- Reviews can reject a heuristic by pointing at the rule.
