# 0038. Every release gets a new version

## Status

Accepted (2026-11-05), implemented. Adopted from TimeTracker (its ADR 0043), owner's decision.

## Context

Every release build carried 0.1.0. Installs could not be told apart: `databee --version` printed the same
version before and after an update, and the owner could not see which build ran. Releases are built by
`bun run release`, by hand or at the end of a step (`.claude/skills/ship-step/SKILL.md`); most commits are
never released on their own.

## Decision

- **Semantic versions, starting at 1.0.0**, the first release under this rule (after 0.1.0). The current
  0.1.0 build is not rebuilt; the next release takes 1.0.0.
- **Every release takes a bump**: `bun run release major|minor|patch|X.Y.Z` (`scripts/release.sh`). A version
  only moves forward; a version with a tag `vX.Y.Z` is never built again. The script needs a clean working
  tree.
- **The version lives in** `package.json`, `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` (and
  `src-tauri/Cargo.lock`); `version_is_the_same_in_cargo_tauri_config_and_package_json` in
  `src-tauri/src/lib.rs` keeps them equal.
- **Build first, then record**: the script writes the version, builds and installs (`scripts/install.sh`),
  then commits "Release X.Y.Z" and tags `vX.Y.Z` locally, never pushed. A failed build puts the version files
  back, so no version is used twice or skipped.
- **Debug builds** keep the version of the last release and are never released.
- **A meaningful bump.** The person releasing (or Claude in ship-step) reads `git log v<last>..HEAD` and picks
  the highest that applies:
  - **major**: a change that breaks how the owner or scripts use the app: a removed or changed CLI command,
    flag or output, `databee://` link, MCP tool or Stage API; a changed format of the config files, presets,
    projects or `keybindings.json` that old files no longer load in; a removed feature or command id.
  - **minor**: new features or visible behavior that keeps existing use working: new commands, settings,
    views, preset or project rules.
  - **patch**: fixes, polish, performance and docs only.
- Rejected: one fixed version (installs can't be told apart; the owner can't see what runs); a build number
  or a date (says nothing about compatibility); deriving the bump from commit prefixes (the commit rules
  forbid prefixes, `.claude/rules/commits.md`); bumping in every commit (most commits aren't released).

## Consequences

- `databee --version` names the build that runs, and `git log v1.2.0..v1.3.0` lists what a release brought.
- Every release adds a commit and a tag to `main`; a release made at the end of a step follows the step's
  commit.
- The bump is a judgment from the log; a wrong one is fixed by the next release, never by moving a tag.
