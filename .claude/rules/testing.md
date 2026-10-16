# Testing

- Every feature or change ships with unit tests for its observable behavior.
- Test through public interfaces: Tauri command results, CLI parsing results,
  rendered UI (Testing Library queries by role/text).
- Never test internals, private helpers, or irrelevant side effects (log output, call counts).
- Rust: `#[cfg(test)] mod tests` next to the code; tests that need a database server are `#[ignore]`
  integration tests in `src-tauri/tests/` against the Docker fixtures (`dev/`).
- UI: `*.test.tsx` next to the component; mock the `src/api` module with `createApiMock()` (`src/test/apiMock.ts`),
  never components or the store.

## Delegation

Keep test code and test output out of the main context:

- **Writing tests:** spawn `write-ts-unit-test` (frontend) or `write-rust-unit-test` (Rust, including
  `mod tests` blocks and Docker fixtures) with the code under test and the behaviors to cover. Never write
  test code inline. Independent writers can run in parallel.
- **Running tests, lint and format checks:** spawn the `ci` agent with the suite (and a filter while
  iterating). Never run `bun run test`, `vitest`, `cargo test`, `cargo clippy` or `bun run lint` in the main
  context.
- **On failures:** send the `ci` lines back to the writer that owns the test (or fix the code under test),
  then run `ci` again.
