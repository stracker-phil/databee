# 0039. A major version needs the owner's approval

## Status

Accepted (2026-11-11), implemented. Amends 0038 (the major bump). Inherited from the family catalog
(release-versions; reference: TimeTracker ADR 0047).

## Context

ADR 0038 made every change that breaks existing use a major bump. DataBee is a personal tool: its only
consumers are the owner and the owner's own scripts, so a major version signals breakage to nobody who needs
the warning. The family catalog moved to the rule below after TimeTracker released a refused command as a
major version the owner did not want.

## Decision

- **Changes that break existing use are minor**, like new features and visible behavior. What counts as
  breaking stays as 0038 lists it: a removed or changed CLI command, flag or output, `databee://` link, MCP
  tool or Stage API; a changed format of the config files, presets, projects or `keybindings.json` that old
  files no longer load in; a removed feature or command id.
- **A major bump needs the owner's explicit approval**, asked before releasing; never chosen from the log alone.
- **Patch** stays as it was: fixes, polish, performance and docs only.
- Rejected: semver's strict major for breakage (signals breakage to consumers DataBee does not have); keeping
  0038 as it was (DataBee would differ from the other apps of the family without a reason).

## Consequences

- `.claude/skills/ship-step/SKILL.md` step 7 picks minor or patch from the log and asks before a major.
- `scripts/release.sh` is unchanged: it takes the bump it is given.
