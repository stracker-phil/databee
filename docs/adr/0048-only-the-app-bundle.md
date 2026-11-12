# 0048. Only the app bundle is built

## Status

Accepted (2026-11-12), owner's decision. Deviates from the family template (`bundle.targets` `app` and `dmg`).

## Context

The family template builds an `.app` and a `.dmg`. DataBee is installed on the owner's Mac by
`scripts/install.sh` and never handed to anyone.

## Decision

Build only the `.app` (`bundle.targets: ["app"]`); never create a `.dmg`. Rejected: a `.dmg` (nobody downloads
DataBee; it only costs build time).

## Consequences

- `scripts/release.sh` and `bun run build:app` build the `.app` alone.
