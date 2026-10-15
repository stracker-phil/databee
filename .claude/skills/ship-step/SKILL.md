---
name: ship-step
description: Finish a development step in databee - run tests and lint, build and launch the app, then commit. Use at the end of every feature or change.
---

1. `bun run test`: fix failures before continuing.
   If the change touches `db/` or `wp/`, also run `bun run test:db` (needs Docker).
2. `bun run lint`: fix all warnings (`cargo fmt --manifest-path src-tauri/Cargo.toml` first).
3. `pkill -x databee; bun run app`, then confirm it runs with `pgrep -x databee`.
   For UI changes, connect to a DB via the CLI (the binary is inside the .app bundle) and check the result
   with `scripts/dev/screenshot.sh <png>`; drive the UI with `scripts/dev/ui.sh <x> <y> [text] [enter]`.
4. `git add -A && git commit` following `.claude/rules/commits.md`.
5. Report to the user: what changed, test results, and that the app launched.
