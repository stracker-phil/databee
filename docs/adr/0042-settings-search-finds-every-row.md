# 0042. Settings search finds every row; one key per command

## Status

Accepted (2026-11-12), implemented. Amends 0031 (the search) and 0036 (shortcut editing, the Help menu, Settings
commands). Inherited from the family catalog (settings-window, keyboard-first, palette-runs-the-menus;
reference: Launcher ADRs 0032, 0045, 0046).

## Context

The Settings search matched page labels and section titles, so a setting was found only through the heading
above it. Shortcuts warned about a key two commands shared but kept both, showed Reset even when a key equaled
its default and listed every command in one group. The palette ranked commands by prefix and substring only.

## Decision

- **Every row is searchable.** Each page lists its rows and titled groups by anchor (`rows`); every `Row` and
  `Group` renders its anchor; `searchIndex.test.tsx` fails when the list and the page differ. Typing replaces
  the page list with the ranked pages and rows; a row hit shows the page and the row's title; picking it opens
  the page scrolled to the row and washes it in the accent once, again on every pick.
- **One ranking** (`src/lib/rank.ts`) for the Settings search and the palette's commands: the label starting
  with the text, a word of it, every word, the letters in order. The palette reads which commands can run once
  per opening.
- **One key per command.** A taken key asks before it moves (Reassign, one save); Reset shows only while a
  key differs from its default; recording the default resets; rows are grouped by where the key works.
- **Family Settings layout**: 190px page list, the current page filled with the accent, inset groups (12px
  corners, 16px padding, 24px apart), no heading that repeats the page title. New pages Status and About; the
  Logging page folds into Status.
- **Commands** open each Settings page (`settings.<page>`), and Help gains Open Log Folder and Show Data Folder.
- Rejected: searching only page and group titles (one hit per page hides the rest); reading rows from the
  rendered pages at runtime (a page not shown can't be searched); a warning for shared keys (two commands
  keep fighting over one key); a separate ranking per list.

## Consequences

- A new setting is a row with an anchor plus an entry in its page's `rows`, or a test fails.
- The Shortcuts page lost its own search field; the Settings search finds each command.
