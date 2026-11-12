# 0040. One tooltip host per window

## Status

Accepted (2026-11-12), implemented. Inherited from the family catalog (tooltip-host; reference: Taskboard
docs/ux.md → Tooltips).

## Context

DataBee explained its icon buttons, status texts and cut-off values with about 90 native `title` attributes.
macOS shows those late (well over a second), small and in the system's style, never on keyboard focus, and
over DataBee's own overlays; text cut off with an ellipsis had no tooltip at all unless a `title` repeated it.
Key hints were spelled into the titles by hand ("New query (⌘T)"), some fixed, so a rebinding in Settings left
them wrong. The other apps of the family show tooltips through one host per window.

## Decision

- **One `TooltipHost` per window** (`src/components/TooltipHost.tsx`, mounted in `App.tsx` and
  `SettingsApp.tsx`) draws one styled bubble for every `data-tooltip`. Components write `data-tooltip`
  instead of `title`, so React keeps it current; a `title` that still appears is moved to `data-tooltip` when
  the pointer reaches it, so the native tooltip never shows. It shows after 450 ms, at once within 600 ms of
  the last one, on keyboard focus, and for ellipsized text; below the control (above if there's no room), up
  to 420px wide, 8px inside the window, line breaks kept; a press, a key, the wheel or leaving hides it.
- **Bindings come from the registry.** A control that runs a catalog command carries `data-command="<id>"`;
  the host appends the command's current binding ("Toggle sidebar (⌃⌘S)"). No tooltip spells a key itself.
- Icon-only controls keep their `aria-label`; Stage anchors read `data-tooltip` where they read `title`.
- Rejected: native `title` (slow, inconsistent, hangs over overlays, no focus or ellipsis tooltips); a
  `<Tooltip>` wrapper per control (touches every control, easy to forget, and each wrapper needs its own
  timing state where one host shares it across the window).

## Consequences

- `docs/ux.md` → Tooltips describes the host; `styles/tooltip.css` styles the bubble with scheme tokens.
- A label that reveals itself on hover (`data-reveals`, the sidebar's table names) opts out of the ellipsis
  tooltip so the two never show together.
- A tooltip is no longer an accessible description; a control's name is its text or `aria-label`.
