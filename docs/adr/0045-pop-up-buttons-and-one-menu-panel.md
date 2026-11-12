# 0045. Pop-up buttons and one menu panel

## Status

Accepted (2026-11-12), implemented, owner's decision. Supersedes 0035 for menus (its fonts and color schemes
stand). Inherited from the family catalog (menus-in-the-page; reference: TimeTracker `PopUpButton`, Taskboard
`src/components/menu/`).

## Context

ADR 0035 kept DataBee's `ContextMenu` and left a shared menu component for later. Native `<select>`s remained
in the toolbar, dialogs, project forms and Settings; they ignore the color schemes and draw macOS's own menu.
The owner called that drift unintended.

## Decision

- One `MenuPanel` draws context menus and pop-up menus: at a point or under a button, flipped and kept inside
  the window; ↑↓ wrapping, Home/End, letter jump, Enter/Space, Esc returning the focus; closed by an outside
  press, scroll, blur or resize.
- `PopUpButton` replaces every native `<select>`: the current choice with a chevron, as wide as its widest
  choice, groups with captions, a check on the current choice.
- Rejected: native selects (they ignore the schemes); a second panel for pop-ups beside the context menu (two
  key and close behaviours).

## Consequences

- No `<select>` in `src/`; a new choice list uses `PopUpButton`.
