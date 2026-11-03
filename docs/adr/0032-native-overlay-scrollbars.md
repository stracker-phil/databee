# 0032. Native overlay scrollbars

## Status

Accepted (2026-11-03), implemented. Adopted from Taskboard (its ADR 0010, scrollbar part).

## Context

DataBee styled every scrollbar (`::-webkit-scrollbar*`): an 8px track with a 6px thumb. Any such rule makes
WebKit draw a permanent bar that takes its own room, whatever macOS's "Show scroll bars" setting says. So
every pane showed bars at rest, kept a gutter for them (`scrollbar-gutter: stable`) and sized layouts around
them (`--scrollbar-size`). Taskboard keeps macOS's own overlay bars and has no scrollbar code.

## Decision

- No `::-webkit-scrollbar` rule and no `--scrollbar-*` token. macOS draws its overlay bars, only while
  scrolling (or always, when the user chose that in System Settings).
- No `scrollbar-gutter`: an overlay bar takes no room, so nothing shifts when a box starts to scroll.
- One `.scroller` class (`overflow: auto; isolation: isolate`) on every scroll box, in place of a long
  selector list in `app.css`. A box narrows the overflow when it scrolls one way only.
- The tab bar scrolls sideways without a bar (`scrollbar-width: none`).
- A scroll box starts below its title row, never under the traffic lights.

Rejected:
- Keeping the styled bars: they are permanent, cost room in every pane and disagree with every other Mac app.
- Thinner styled bars that fade by script: WebKit has no hook for "scrolling now" short of scroll listeners
  on every box, and it would still be a web imitation of the native bar.
- Keeping `scrollbar-gutter: stable` for users who show bars always: then the gutter shows empty for
  everyone else; macOS already relayouts for those users.

## Consequences

- An overlay bar may cover the last pixels of a row or column while scrolling; that is how every Mac app
  behaves.
- New scroll boxes must carry `.scroller`, or a nested bar can stay behind while an outer box scrolls.
