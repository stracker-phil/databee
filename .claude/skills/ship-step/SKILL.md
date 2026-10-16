---
name: ship-step
description: Finish a development step in DataBee - run tests and lint, build and launch the app, then commit. Use at the end of every feature or change.
---

1. `bun run test`: fix failures before continuing.
   If the change touches `db/` or `wp/`, also run `bun run test:db` (needs Docker).
2. `bun run lint`: fix all warnings (`cargo fmt --manifest-path src-tauri/Cargo.toml` first).
3. `bun run build:app`, then `scripts/dev/launch.sh [CLI args]`. This restarts the app on the user's terminal
   screen without taking focus; never use `bun run app` or `open` (they steal focus on the user's other screen).
   Connect via CLI args, e.g. the Docker fixture (`docker compose -f dev/docker-compose.yml up -d --wait`):
   `scripts/dev/launch.sh mysql://root:root@127.0.0.1:33306/wp_test`, or a local site the user named.
   Drive the UI with `scripts/dev/js.sh '<js>'` (fire-and-forget; wrap code in an IIFE, the store is `window.databee`) and check it with
   `scripts/dev/screenshot.sh <png>`. Never use AppleScript clicks or keystrokes; they need focus.
4. Update the affected `docs/` page if behavior, commands, files or extension points changed.
5. `git add -A && git commit` following `.claude/rules/commits.md`.
6. Report to the user: what changed, test results, and that the app launched.
