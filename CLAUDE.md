# databee

Minimalistic SQL client for personal use (mainly MySQL/MariaDB for WordPress dev, SQLite later).

## Stack
- Backend: Rust (rustup stable, `src-tauri/`)
- UI: Tauri 2 + React + TypeScript (`src/`), bun as package manager
- Target: macOS 27, Apple Silicon only

## Product constraints
- Primary entry point is a CLI: another app/script launches databee with connection details.
- Databases are local but may run in Docker (host port mapping, socket-less TCP).
- Secrets (passwords) live in the macOS Keychain, never in files or logs.

## Commands
- `bun run test` — all tests (Vitest + cargo test)
- `bun run lint` — tsc + clippy (warnings are errors)
- `bun run app` — debug build of the .app bundle and launch it

## Workflow
Finish every step with the `ship-step` skill: test → lint → launch → commit.
See `.claude/rules/` for testing and commit rules.
