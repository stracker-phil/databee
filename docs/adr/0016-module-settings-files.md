# 0016. Module settings live in the module's own JSON file

## Status

Accepted

## Context

Stage settings inside `settings.json` forced the core `Settings` type, its sanitizer and the settings command
to know about Stage, and a bad Stage value could quarantine every core setting.

## Decision

A module persists its settings in `<config>/<module>.json` through the shared `JsonStore`, with a one-time
migration from the old location. The core `settings.json` never carries module keys.

## Consequences

- Modules evolve their settings independently, and removing one leaves no orphan schema in the core.
- Settings are spread over several files.
- Each module provides its own settings commands.
