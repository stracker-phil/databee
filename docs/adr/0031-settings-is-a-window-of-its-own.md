# 0031. Settings is a window of its own

## Status

Accepted (2026-11-03), implemented. Adopted from Taskboard (its ADR 0010, Settings part).

## Context

Settings was a modal sheet in the main window: it covered the work it configures, had no search, and pages
read the main window's store directly. Taskboard's Settings is a native window with a searchable page list.

## Decision

- Settings is a second webview window (`window/settings.rs`, label `settings`, 880×720, not maximizable),
  built hidden on first use. `open_settings(page)` shows it and brings it to the front; with a page or section
  id it emits `open_settings` to switch. Closing hides it, so it reopens on its last page.
- One open path: the store's `openSettings` (⌘, menu item, toolbar button, Stage) calls Rust.
- Each window has its own store. Rust emits `settings_changed` to every window after every settings or
  keybindings update; each reloads, so appearance and keys apply everywhere at once.
- The nav has a search field over page labels, nav groups and section titles. Every settings group has a
  title and an anchor, listed as the page's `sections`.
- Stage opens and closes the window and reports it as open, but reaches no anchor inside it (owner's choice,
  2026-11-03); a second Stage page can come later.

Rejected:
- Keeping the sheet and adding search: it still covers the main window and can't stay open beside it.
- One store shared across windows through Rust: every page would need its own commands; reloading on an
  event is enough for settings that change rarely.
- Full Stage support for the second window now: Stage's page protocol assumes one page, a larger change.

## Consequences

- Settings can stay open next to the main window; changes show in both at once.
- Stage presentations can open Settings on a page but can't spotlight its controls.
- Main window code must not assume Settings lives in its DOM.
