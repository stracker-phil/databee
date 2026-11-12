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
   (`.claude/rules/stable-controls.md`), and run `scripts/dev/e2e.sh` on the fixture: it fails when the
   sidebar, toolbar, main area, first grid row or a clicked control moved. When the checks are done, quit the debug app fully
   (`scripts/stop-app.sh "src-tauri/target/debug/bundle/macos/DataBee (Debug).app/Contents/MacOS/databee"`).
5. Update the affected `docs/` page if behavior, commands, files or extension points changed; add an ADR for
   a new decision (`docs/adr/`; never edit an old one: a new record supersedes or amends it). Check that no
   real data slipped into code, docs or screenshots (`.claude/rules/demo-data.md`).
6. Stage the step's files by path (check `git status` for stray files), then commit following
   `.claude/rules/commits.md`. Never push.
7. Choose the bump from `git log v<last>..HEAD` (`git describe --tags --abbrev=0` names the last; with no
   tag yet, release `1.0.0`) per ADR 0038 and 0039: minor for new features, visible behavior and changes
   that break existing use, patch otherwise. Major only with the owner's explicit approval, asked first.
   Run `bun run release major|minor|patch` (or `bun run release 1.0.0`; in the background, it takes minutes): `scripts/install.sh` installs the release build as
   `~/Applications/DataBee.app` and `databee`, and restarts the release app if it runs, so the user's app
   always matches the last commit; then the script commits "Release X.Y.Z" and tags `vX.Y.Z` (never pushed).
8. If this step added an ADR that other apps of the family could share, ask the owner whether to propose it
   for the family catalog (the user-level `app-blueprint` skill, mode `record`). Only ask: never edit the
   catalog yourself, and drop it when the owner says no.
9. Report to the user: what changed, the `ci` summary, open review questions, that the debug app was checked and
   quit, and that the release app was installed, with its new version.
