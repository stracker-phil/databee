---
description: Frontend test files are written by the write-ts-unit-test agent
paths:
  - "src/**/*.test.ts"
  - "src/**/*.test.tsx"
  - "src/test/**"
  - "src/**/apiMock.ts"
  - "src/test-setup.ts"
---

Don't edit these files inline, not even helpers, fixtures or mock defaults. Spawn the `write-ts-unit-test`
agent with the code under test and the behaviors to cover; it knows the suite's conventions. It never runs
what it writes: verify its output with the `ci` agent.
