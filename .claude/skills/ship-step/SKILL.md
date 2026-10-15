---
name: ship-step
description: Finish a development step in databee - run tests and lint, build and launch the app, then commit. Use at the end of every feature or change.
---

1. `bun run test`: fix failures before continuing.
   If the change touches `db/` or `wp/`, also run `bun run test:db` (needs Docker).
2. `bun run lint`: fix all warnings (`cargo fmt --manifest-path src-tauri/Cargo.toml` first).
3. `bun run build:app`, then `scripts/dev/launch.sh [CLI args]`. This restarts the app on the user's terminal
   screen without taking focus; never use `bun run app` or `open` (they steal focus on the user's other screen).
   Connect via CLI args, e.g. `echo root | scripts/dev/launch.sh mysql://root@127.0.0.1:52958/db --password-stdin`.
   Drive the UI with `scripts/dev/js.sh '<js>'` (e.g. `document.querySelector(...).click()`) and check it with
   `scripts/dev/screenshot.sh <png>`. Never use AppleScript clicks or keystrokes; they need focus.
4. `git add -A && git commit` following `.claude/rules/commits.md`.
5. Report to the user: what changed, test results, and that the app launched.
