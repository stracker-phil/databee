# 0035. DataBee keeps its own look

## Status

Rejected (2026-11-03), owner's decision. Taskboard moved to a native look (its ADRs 0010, 0028, 0034);
DataBee does not adopt it.

## Context

Taskboard uses the system font, two native palettes and one in-page menu component. DataBee bundles Geist
and JetBrains Mono, offers editor color schemes with accents
([ADR 0024](0024-color-schemes.md)) and renders every database value in mono.

## Decision

Keep the bundled fonts, the color schemes and the accents. Adopt only the native parts that don't change the
look: macOS overlay scrollbars ([ADR 0032](0032-native-overlay-scrollbars.md)) and Settings as a window of
its own ([ADR 0031](0031-settings-is-a-window-of-its-own.md)).

Rejected:
- The system font for chrome and data: DataBee is a data tool; mono for every value is a principle
  (`docs/ux.md` → Principles), and the bundled fonts keep widths the same on every Mac.
- Two native palettes instead of the schemes: developers read code in these schemes all day; matching their
  editor is the point of the setting.
- Taskboard's in-page menu component now: DataBee's `ContextMenu` already shows keys
  (`MenuItem.command`); submenus and checks come when a menu needs them.

## Consequences

- Visual decisions keep following `docs/ux.md`, not Taskboard's.
- Scheme files stay the only place for color literals.
