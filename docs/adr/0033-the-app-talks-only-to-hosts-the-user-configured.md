# 0033. The app talks only to hosts the user configured

## Status

Accepted (2026-11-03). Adopted from Taskboard (its ADR 0013, inherited from Daybook).

## Context

DataBee holds connections to databases. A dependency that brings a client of its own (an updater, crash
reporting, analytics, fonts from a CDN) could send data out without anyone noticing. What leaves the Mac was
not written down anywhere.

## Decision

- The only outgoing connections are the database connections the user configured (`sqlx` MySQL, host and
  port from the connection, `db/mysql/`). There is no HTTP client in the crate.
- The only listening socket on the network stack is the Stage API, bound to `127.0.0.1` only
  (`stage/mod.rs` `bind`). The single-instance socket is a Unix socket in the config dir; the MCP server
  speaks over stdio.
- The page loads nothing from outside the app: the CSP allows `connect-src ipc:` and
  `http://ipc.localhost` only ([ADR 0030](0030-hardened-runtime-strict-csp-no-navigation.md)); fonts are
  bundled.
- Opening a site's URL in the browser (`commands.rs` `open_url`, web URLs only) is not network access by the
  app; it goes through `tauri-plugin-opener`.
- A new host or listener needs an ADR and a row in `docs/adapting.md` → Network.

Rejected:
- An allow-list in code like Taskboard's `net.rs` `HOSTS`: DataBee's hosts are whatever the user types into
  a connection; there is no fixed list to check against.
- An update check or crash reporting: DataBee is one developer's tool built from source; both would be the
  first data to leave the Mac unasked.

## Consequences

- What leaves the Mac is reviewable in one table.
- Adding a dependency that opens connections (HTTP, telemetry) is a decision, not a detail.
