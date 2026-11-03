# 0028. The debug build is a separate app with demo data

## Status

Accepted (2026-11-03), implemented. Adopted from Taskboard (its ADRs 0011 and 0026); amends
[0005](0005-own-single-instance-socket.md) (one socket per build) and [0004](0004-passwords-in-secret.md)
(where passwords are kept in debug builds).

## Context

The debug build had its own name and bundle id but shared the release app's config folder, data, socket,
lock, Keychain service and Stage port. A debug launch handed its arguments to the running release app or
stopped it, and half-finished code ran on the owner's connections and passwords. Two Stage servers could not
listen on one port.

## Decision

- Debug builds (`bun run build:app`, `tauri.debug.conf.json`) are a second app: `DataBee (Debug)`,
  `com.philippstracker.databee.debug`, URL scheme `databee-debug`. `lib.rs` (`IDENTIFIER`, `APP_NAME`,
  `LINK_SCHEME`) has tests that compare them with both Tauri configs. Everything that hangs off the
  identifier is separate: config folder (connections, history, projects, settings, Stage settings,
  snapshots), socket, lock and logs. The Stage port is 27185 instead of 27184.
- Debug builds never touch the Keychain: `secrets::app_store()` is an in-memory store that starts with the
  demo passwords. Release keeps the Keychain service `databee`; renaming it would lose every saved password.
- On its first launch (no `connections.json`) the debug app adds the demo connections: the Docker fixtures
  from `dev/` (`test_support::demo_connections`), so tests, the debug app and screenshots share one source of
  made-up data.
- Links: the debug app registers `databee-debug://` and reads such a link as the `databee://` address it
  names. Addresses inside both apps (status bar, palette, MCP, CLI) stay `databee://`.
- Scripts act on one build: `launch.sh` restarts only the debug app and refuses a bundle older than the code;
  `stop-app.sh` stops the build its path belongs to; `js.sh` and `screenshot.sh` reach only the debug app;
  `install.sh` restarts only the release app and replaces `~/.local/bin/databee` only when it is its own link.

Rejected:
- Debug passwords in a file: plain-text secrets on disk, and still the owner's connections.
- Keeping one identity and stopping the release app for every debug launch: it interrupts the owner's work
  and still runs new code on real data.
- Addresses in the build's scheme everywhere: every address in tests (which compile as debug) would change,
  for little gain; a link copied from the debug app opens the release app instead.

## Consequences

- Trying a change never touches the owner's connections, data or Keychain, and both apps run side by side.
- The debug app needs the Docker fixtures (`docker compose -f dev/docker-compose.yml up -d --wait`) for its
  demo connections.
- Each build is its own entry in Privacy & Security, granted once.
