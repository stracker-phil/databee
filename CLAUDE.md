# databee

Minimalistic SQL client for personal use.

## Stack
- Backend: Rust (via rustup toolchain)
- UI: Tauri + TypeScript
- Target: macOS 27, Apple Silicon only

## Rules
- Every feature/change ships with unit tests covering observable behavior.
  Never test internals or irrelevant side effects (e.g. log output).
- Build and launch the app after each step.
- Commit at the end of every step. First line ≤ 60 chars, concise;
  add a body only when it adds value.
