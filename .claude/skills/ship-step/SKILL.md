---
name: ship-step
description: Finish a development step in DataBee - clean up and review the code, run tests and lint through the ci agent, build and launch the app, then commit. Use at the end of every feature or change.
---

1. Run the `clean-up-code` skill on the uncommitted changes.
2. Spawn one `review-code-cleanliness` agent per changed source file (`review CHANGES in <file>`), in
   parallel. Apply the findings that hold; weigh the `Consider` questions and mention open ones in the report.
3. Run `cargo fmt --manifest-path src-tauri/Cargo.toml`, then spawn the `ci` agent for "All unit tests" and
   "Lint", one after the other: both regenerate `src/api/types.gen.ts` and `src/stage/types.gen.ts`, and two
   runs at once corrupt them. If the change touches `db/`, `rules/`, `document/` or another module with `server_tests`, also run "Integration" (needs Docker). Fix
   failures (test fixes go back to the `write-*-unit-test` agent) and run `ci` again until green.
4. `bun run build:app`, then `scripts/dev/launch.sh [CLI args]`. This restarts the debug app on the user's terminal
   screen without taking focus; never use `bun run app` or `open` (they steal focus on the user's other screen).
   Connect via CLI args, e.g. the Docker fixture (`docker compose -f dev/docker-compose.yml up -d --wait`):
   `scripts/dev/launch.sh mysql://root:root@127.0.0.1:33306/wp_test` (`wp_write` for writes). The debug app
   runs on demo data only, never on the user's own sites (ADR 0028).
   Drive the UI with `scripts/dev/js.sh '<js>'` (fire-and-forget; wrap code in an IIFE, the store is `window.databee`) and check it with
   `scripts/dev/screenshot.sh <png>`. Never use AppleScript clicks or keystrokes; they need focus.
   For UI changes, click every new or changed control twice and check nothing beside it moved
   (`.claude/rules/stable-controls.md`).
5. Update the affected `docs/` page if behavior, commands, files or extension points changed; add an ADR for
   a new decision (`docs/adr/`; never edit an old one: a new record supersedes or amends it). Check that no
   real data slipped into code, docs or screenshots (`.claude/rules/demo-data.md`).
6. Stage the step's files by path (check `git status` for stray files), then commit following
   `.claude/rules/commits.md`. Never push.
7. Choose the bump from `git log v<last>..HEAD` (`git describe --tags --abbrev=0` names the last; with no
   tag yet, release `1.0.0`) per ADR 0038: major if existing use breaks, minor for new features or visible
   behavior, patch otherwise. Run `bun run release major|minor|patch` (or `bun run release 1.0.0`; in the
   background, it takes minutes): `scripts/install.sh` installs the release build as
   `~/Applications/DataBee.app` and `databee`, and restarts the release app if it runs, so the user's app
   always matches the last commit; then the script commits "Release X.Y.Z" and tags `vX.Y.Z` (never pushed).
8. Report to the user: what changed, the `ci` summary, open review questions, that the debug app launched and
   that the release app was installed, with its new version.
