# 0021. Every view has an address

## Status

Accepted (2026-10-17). Being implemented in U3; used outside the app from U14.

## Context

Navigation state lived in tab objects and in a WordPress-only route type (`WpRoute`) with its own history.
There was no way to name a view: not to copy it, link to it from a terminal or an agent, restore it, or
move back and forward through table tabs.

## Decision

Every view is an `Address` (Rust `src-tauri/src/address.rs`, mirrored in TypeScript):

```
databee://<slug>/<db>/                      home (readout)
databee://<slug>/<db>/<table>               rows of a table
databee://<slug>/<db>/<table>?col=val&…     rows with a structured filter
databee://<slug>/<db>/<table>/<key>         object (Document view, the default)
databee://<slug>/<db>/<table>/<key>?view=rows
databee://<slug>/<db>/q/<saved-query-id>    saved query result
databee://<slug>/<db>/?find=<value>         find everywhere
databee://<slug>/<db>/?project              project page
```

- `<slug>` is a stable, unique, user-editable name on each saved connection, because DDEV names every
  database `db`.
- `<table>` is the real table name with its prefix. Entity names are accepted as input shorthand and
  resolved, never stored in a canonical address.
- `<key>` is the primary-key value; composite keys use the filter form.
- The view is a query parameter, so one form covers Document and Rows.
- Back and forward history is a list of addresses per tab. Free SQL filters are not addressable.

Separate URL forms per view were rejected; keeping routes in UI state only was rejected because nothing
outside the app could reach a view.

## Consequences

- The status bar, deep links, the CLI and the MCP server all speak the same string.
- Renaming a slug breaks old addresses; the slug is editable but defaults to something stable.
- `Address::parse` and `Display` must round-trip every form; the TypeScript port is checked against a
  fixture the Rust tests write.
