# AGENTS.md

Instructions for a coding agent that clones, builds or adapts DataBee. Humans: see [README.md](README.md).

DataBee is one developer's personal tool, published as source. There are no releases, binaries or support.
As shipped it builds and runs on **macOS on Apple Silicon** only; everything else is a port.

## Repository facts

- Tauri 2 app: Rust backend in `src-tauri/`, React 19 + TypeScript frontend in `src/`, bun as package manager.
- Rust owns all logic; React only renders. Read [docs/architecture.md](docs/architecture.md) before changing
  code, and the page of the layer you touch ([docs/README.md](docs/README.md) is the index).
- [CLAUDE.md](CLAUDE.md) and [.claude/rules/](.claude/rules/) are the maintainer's conventions (tests,
  commits, presets). Follow them in this repo; in a fork, keep what serves you.
- IPC types in `src/api/types.gen.ts` are generated from Rust with ts-rs (`bun run types`); never edit them.

## Prerequisites (macOS)

| Tool | Check | Notes |
|---|---|---|
| Xcode Command Line Tools | `xcode-select -p` | `xcode-select --install` |
| rustup, stable toolchain | `rustup show` | `rust-toolchain.toml` pins `stable` with rustfmt and clippy. A Homebrew rustup is keg-only: put `/opt/homebrew/opt/rustup/bin` on `PATH` |
| bun | `bun --version` | |
| Docker (optional) | `docker info` | only for the integration tests (`bun run test:db`) |

## Clone and build

```sh
git clone https://github.com/stracker-phil/databee.git
cd databee
bun install
bun run build:app        # debug bundle: src-tauri/target/debug/bundle/macos/DataBee (Debug).app
bun run release          # release bundle, installed for the user (below)
```

The first Rust build takes several minutes. `bun run release` ends with `scripts/install.sh`: it links
`~/Applications/DataBee.app` to the release bundle and `~/.local/bin/databee` through that app, and restarts
a running release app on the new build. Debug builds are never installed; `scripts/dev/launch.sh` runs one by its
path, beside the release app: it has its own data and starts with the Docker fixtures as connections. Builds are signed with an Apple Development certificate when the Mac has one (a free Apple ID in Xcode), so the Keychain trusts every release build after one "Always Allow"; without one, run `scripts/create-signing-cert.sh` once, and each new build asks once for saved
passwords.

## Verify

| Command | Expect |
|---|---|
| `bun run test` | Vitest and Rust unit tests pass |
| `bun run lint` | exits 0: types drift check, `tsc`, ESLint, `clippy -D warnings` |
| `bun run test:db` | starts MariaDB 11.4 (`127.0.0.1:33306`) and MySQL 8.4 (`127.0.0.1:33307`), user `root`, password `root`, seeded from `dev/fixture.sql`; the integration tests pass |

Try the app against the fixtures: `scripts/dev/launch.sh mysql://root:root@127.0.0.1:33306/wp_write`. Write only to the fixture databases (`wp_write`,
`databee_test`), never to a user's own databases. [docs/testing.md](docs/testing.md) covers the suites,
the fixtures and scripts that drive the running app.

## Adapt

[docs/adapting.md](docs/adapting.md) lists every place that is specific to macOS, to the name or to the
WordPress presets, with the replacement for Linux and Windows, recipes (add a database engine, swap the
secret store, remove Stage or the presets) and when a port counts as done.

Starting points for common requests:
- **Linux:** replace the Keychain with the `keyring` crate, move the config dir to `dirs::config_dir()`,
  map `Mod` to Ctrl, drop the macOS window chrome and traffic-light padding, add `deb`/`appimage` targets.
  The Unix socket handoff works as is.
- **Windows 10/11:** as Linux, plus a named pipe instead of the Unix socket and `#[cfg(unix)]` around file
  permissions. Windows 8 and older cannot run it: Tauri 2 needs the WebView2 runtime, which no longer
  supports them.
- **Rename the app:** the Identity table in docs/adapting.md (bundle id, binary, URL scheme, socket, paths).
- **Your own plugin's tables:** a preset file, no code ([docs/presets.md](docs/presets.md)).

When done, run the Verify table on the target platform and update the docs pages your change affects.
