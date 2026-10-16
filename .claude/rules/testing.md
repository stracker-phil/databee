# Testing

- Every feature or change ships with unit tests for its observable behavior.
- Test through public interfaces: Tauri command results, CLI parsing results,
  rendered UI (Testing Library queries by role/text).
- Never test internals, private helpers, or irrelevant side effects (log output, call counts).
- Rust: `#[cfg(test)] mod tests` next to the code; DB-dependent tests use SQLite in-memory
  or are marked `#[ignore]` with a note on the Docker service they need.
- UI: `*.test.tsx` next to the component; mock the `src/api` module with `createApiMock()` (`src/test/apiMock.ts`),
  never components or the store.
