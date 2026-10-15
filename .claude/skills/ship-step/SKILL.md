---
name: ship-step
description: Finish a development step in databee - run tests and lint, build and launch the app, then commit. Use at the end of every feature or change.
---

1. `bun run test` — fix failures before continuing.
2. `bun run lint` — fix all warnings.
3. Quit any running instance (`pkill -x databee`), then `bun run app` to build and launch.
   Confirm the process runs with `pgrep -x databee`.
4. `git add -A && git commit` following `.claude/rules/commits.md`.
5. Report to the user: what changed, test results, and that the app launched.
