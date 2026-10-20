# 0025. A standalone, read-only MCP server (`databee mcp`)

## Status

Accepted (2026-10-17), implemented.

## Context

Coding agents working on a WordPress plugin want the same answers a developer gets in DataBee: an order's
document, which rows reference a value, what a saved query returns. Stage's HTTP API drives the window for
presentations and needs the app running; it is not a data interface. Agents should not be able to write to
the user's databases.

## Decision

`databee mcp` runs an MCP server over stdio, unrelated to Stage. Data tools (documents, rows, references,
saved queries, find) connect through the core library: saved connections, Keychain passwords and project
rules, with or without the app running. UI tools (open an address, run a command) reach the running app
through the single-instance socket and report when it is not there. The server is read-only: no tool
writes to a database, including plans and actions. Reusing Stage's API was rejected because it is optional,
presentation-specific and needs the window; a write-capable server was rejected because it would bypass the
review step of [0007](0007-plan-review-apply-undo.md).

## Consequences

- Agents and the app share addresses ([0021](0021-addresses.md)) and rules, so an agent's answer can be
  opened in the app.
- The core library must work headless: no Tauri state on the data path.
- Writes stay a human action in the app.
