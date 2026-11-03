# 0037. Closing the window keeps the app running

## Status

Accepted (2026-11-03), implemented. Adopted from Taskboard (its ADR 0016), owner's decision.

## Context

⌘W closed DataBee's tab and did nothing without one; the red light closed the window and with it the app, so
sessions, the query history writer and the Stage API ended with it. Taskboard and most Mac apps keep running
when their window closes. `scripts/install.sh` restarted the app after an install, always with its window
shown.

## Decision

- Closing a window (the red light, ⌘W without a tab) hides it. DataBee keeps running, with its sessions,
  until Quit (⌘Q, the app menu, `databee quit`). `RunEvent::ExitRequested` without an exit code is ignored.
- ⌘W closes the active tab; with none it hides the window (`window.hide`, a Rust command of the catalog,
  [0036](0036-commands-live-in-a-rust-catalog.md)). With Settings in front, ⌘W hides Settings.
- A Dock click (`RunEvent::Reopen`), `databee`, `databee open` and links show the window again.
- `databee status [--json]` reports whether DataBee runs and its window shows; `databee --hidden` starts it
  hidden. `scripts/install.sh` restarts a hidden app hidden.
- The table live check pauses while the window is hidden (it already pauses without focus).

Rejected:
- A menu-bar item to reopen the window: one more surface; the Dock icon stays while the app runs.
- Quitting when the last window closes (the old behavior): it drops open sessions and the window's state
  for a key that most often means "close this".
- A new `window.closeTabOrWindow` id for ⌘W: it would lose users' rebindings of `tabs.close`; the existing
  command got the fallback instead.

## Consequences

- The Dock icon stays while the app runs, also with every window hidden.
- Connections stay open while the window is hidden.
