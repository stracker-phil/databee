# 0041. Motion tokens and text contrast in every scheme

## Status

Accepted (2026-11-12), implemented. Amends 0024 (the text levels become scheme tokens). Inherited from the
family catalog (motion-and-stable-controls, color-palettes; reference: Taskboard `src/styles/motion.css`,
`src/styles/contrast.test.ts`).

## Context

DataBee wrote durations as literals (`.12s`, `.14s`, `1.6s`) and `docs/ux.md` put the motion tokens off until
a glide was needed, so macOS's Reduce motion had no effect. Its two dimmed text levels were one fixed mix for
every scheme (`--o2` 62%, `--o3` 40% of `--text`); nothing checked their contrast, and in the pale editor
schemes the meta text fell to 1.7:1.

## Decision

- **Durations are tokens** in `src/styles/motion.css` (`--motion-fast` 120ms, `--motion-base` 150ms,
  `--motion-slow` 200ms, `--motion-highlight` 1600ms, `--motion-spin` 900ms, `--ease-out`, `--ease-in-out`),
  with a `prefers-reduced-motion` block and `.motion-off`. `src/styles/motion.test.ts` fails on a literal
  duration anywhere else. The glide helpers of `src/lib/motion.ts` are ported when a change first needs one.
- **Text keeps its contrast in every scheme**: each scheme sets `--o2-mix` and `--o3-mix` (68% and 52% by
  default, more in Ayu Light, Catppuccin Latte, Nord and Solarized), and `src/styles/contrast.test.ts` checks
  4.5:1 for `--text` and 3:1 for `--o2`/`--o3` on `--bg`, `--chrome` and `--pop`. Solarized Light's `--text`
  moves from base01 `#586e75` to `#566c73`, the smallest step that reaches 4.5:1 on its chrome.
- Rejected: keeping literals until a glide is needed (Reduce motion stays ignored, durations drift); one mix
  for all schemes set high enough for the palest (the dark schemes lose their hierarchy); changing the editor
  schemes' own text and surface colors (they are meant to match the editors).

## Consequences

- Secondary and meta text are darker than before in every scheme, most in the pale ones.
- A new scheme fails a test until its text levels reach the targets.
