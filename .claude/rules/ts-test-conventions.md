---
description: Frontend (Vitest) test writing conventions
paths:
  - "src/**/*.test.ts"
  - "src/**/*.test.tsx"
  - "src/test/**"
  - "src/test-setup.ts"
---

Hard rules about how to write frontend unit tests. Violations waste time and money.

**Why:** Vitest tests written outside the `write-ts-unit-test` agent drift from the suite's conventions
(colocation, the `src/api` mock as the only mock, shared fixtures, `describe`/`it` behavior sentences,
assertions on visible output and boundary calls).

**How to apply:** Whenever frontend tests, fixtures or the API mock need to be written or updated, spawn the
`write-ts-unit-test` agent with full context (the code under test and the behaviors to cover). Do not write
test code inline, not even helpers, fixtures or mock defaults. That agent never runs what it writes: verify
its output with the `ci` agent.
