---
name: ship-step
description: Finish a development step in DataBee - clean up and review the code, run tests and lint through the ci agent, build and launch the app, then commit. Use at the end of every feature or change.
---

1. Run the `clean-up-code` skill on the uncommitted changes.
2. Spawn one `review-code-cleanliness` agent per changed source file (`review CHANGES in <file>`), in
   parallel. Apply the findings that hold; weigh the `Consider` questions and mention open ones in the report.
3. Run `cargo fmt --manifest-path src-tauri/Cargo.toml`, then spawn the `ci` agent for "All unit tests" and
   "Lint" (in parallel). If the change touches `db/`, `rules/`, `document/` or another module with `server_tests`, also run "Integration" (needs Docker). Fix
   failures (test fixes go back to the `write-*-unit-test` agent) and run `ci` again until green.
4. `bun run build:app`, then `scripts/dev/launch.sh [CLI args]`. This restarts the app on the user's terminal
   screen without taking focus; never use `bun run app` or `open` (they steal focus on the user's other screen).
   Connect via CLI args, e.g. the Docker fixture (`docker compose -f dev/docker-compose.yml up -d --wait`):
   `scripts/dev/launch.sh mysql://root:root@127.0.0.1:33306/wp_test`, or a local site the user named.
   Drive the UI with `scripts/dev/js.sh '<js>'` (fire-and-forget; wrap code in an IIFE, the store is `window.databee`) and check it with
   `scripts/dev/screenshot.sh <png>`. Never use AppleScript clicks or keystrokes; they need focus.
5. Update the affected `docs/` page if behavior, commands, files or extension points changed.
6. Stage the step's files by path (check `git status` for stray files), then commit following
   `.claude/rules/commits.md`.
7. Report to the user: what changed, the `ci` summary, open review questions, and that the app launched.
