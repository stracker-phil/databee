# 0046. The debug seed runs once, no real-source switches

## Status

Accepted (2026-11-12). The owner left the choice to Claude, the only user of the debug build. Deviates from the
family catalog (debug-build-on-demo-data: `dev seed --yes`, `<APP>_REAL_<SOURCE>`).

## Context

The family's debug builds start over with `<app> dev seed --yes` and switch an outside source to the real one
with an environment variable. DataBee's debug build adds the Docker fixture connections when
`connections.json` is missing (`lib.rs` `seed_demo`); its only outside sources are database servers the user
types in, and its writes go to the fixtures.

## Decision

- Keep the seed as it is: it runs when `connections.json` is missing. Starting over is deleting the debug app's
  data folder (`~/Library/Application Support/com.philippstracker.databee.debug/`) with the app quit.
- No `DATABEE_REAL_<SOURCE>` switch: there is no faked source to switch.
- Rejected: a `databee dev seed --yes` subcommand (one more CLI path for a folder delete); real-source switches
  (nothing to switch).

## Consequences

- `docs/testing.md` says how to start over.
