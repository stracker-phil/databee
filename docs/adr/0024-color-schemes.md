# 0024. Color schemes are complete token sets; no color literal outside a scheme

## Status

Accepted (2026-10-17), implemented (the scan is `src/styles/colors.test.ts`). Amended 2026-10-20.

One exception to "color comes only from the accent and value styles": error chrome uses the value-style
token `--style-red` (for example `.btn-danger`, `.form-error`, `.toast.error`, `.vt-error`, `.action-error`
in `src/styles/app.css`). The per-database accent (the project's `accent` field; `project_accents` and
`project_set_accent` in `src-tauri/src/rules/commands.rs`) is consistent with this record: it picks one of
the settings' accents, so the color still comes from the scheme.

## Context

`theme.css` defined light tokens once and dark tokens twice (media query and `[data-theme="dark"]`), and
components carried their own literals, status tints, type tints, chart and category colors. A missing dark
value only showed when someone looked, and adding a theme meant hunting literals across the CSS and TSX.

## Decision

A color scheme is a complete set of tokens in its own file (`src/styles/schemes/<id>.css`): surfaces
(`--chrome`, `--bg`, `--pop`), fields (`--field`, `--field-line`), `--text`, `--line`, `--band`,
`--row-hover`, `--shadow`, `--scrim`, the accent hues and the value-style colors. Settings pick one scheme
for light and one for dark; the defaults are Ayu Light and Ayu Mirage. `theme.css` holds only
scheme-independent derived tokens (opacity levels, accent mixes, sizes, fonts). No color literal appears
outside a scheme file; a Vitest scan of `src/` enforces it. Status colors are gone: color comes only from the
accent (interactive things) and from value styles that a project or preset configures. Keeping light/dark
overrides in one file was rejected because a forgotten token only fails visually.

## Consequences

- A second scheme exposes every missing token immediately; adding a scheme is adding one file.
- Every new style needs a token first, which slows one-off tweaks on purpose.
- Old tokens survive only as marked aliases until the components that use them are rewritten.
- The design language is documented in [`docs/ux.md`](../ux.md).
