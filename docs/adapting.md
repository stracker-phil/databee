# Adapting DataBee

DataBee is written so that a coding agent can take it over. Clone the repo, open it in Claude Code (or a
similar tool), and describe what you need:

```text
Port DataBee to Linux (x86_64, GNOME). Keep the features, replace the Keychain with libsecret.
```
```text
Add SQLite support: open .sqlite files from the CLI and the connect screen.
```
```text
Rebrand the app as "ShopLens" under com.example.shoplens and drop the Stage integration.
```
```text
Remove the WordPress presets; I only need the plain SQL client.
```

`CLAUDE.md` is loaded automatically. It points the agent to the rules in `.claude/rules/` and to these docs.
The `ship-step` skill (`.claude/skills/ship-step/`) runs after every step: it cleans up the code, runs
tests, lint, a build and a launch, then commits. Code cleanup is the `clean-up-code` skill
(`.claude/skills/clean-up-code/`), and it runs before every commit. Subagents in `.claude/agents/` write the
tests, run the suites (on Haiku) and review changed files, so the main agent's context stays small. This page
lists what such changes touch.

## Identity

The app's name and ids live in a few places. Change them together.

| What | Where | Note |
|---|---|---|
| Product name (window title, bundle `DataBee.app`, app menu) | `src-tauri/tauri.conf.json` `productName` and window `title`; `menu.rs`; `index.html`; `Toolbar.tsx`; `stage/mod.rs` `NAME` | |
| Bundle identifier `com.philippstracker.databee` | `tauri.conf.json` `identifier`; `lib.rs` `IDENTIFIER`; `scripts/dev/js.sh` (socket path) | Also names the config dir and the log dir. Changing it starts with empty settings. |
| Binary and CLI command `databee` | Cargo package `name` in `src-tauri/Cargo.toml`; `cli.rs` `name`; `scripts/install-cli.sh`; `scripts/dev/launch.sh` | The Rust lib is `databee_lib`, also used by `main.rs` and `tests/` |
| Keychain service `"databee"` | `secrets/keychain.rs` `SERVICE` | Changing it loses saved passwords |
| Socket file `databee.sock` | `lib.rs`, `scripts/dev/js.sh` | |
| Address scheme `databee://` | `src-tauri/src/address/mod.rs` `SCHEME`; `src/lib/address.ts` `SCHEME`; `src/test/fixtures/addresses.json` | Copied and saved addresses stop parsing when it changes |
| Deep-link URL scheme `databee` | `tauri.conf.json` `plugins.deep-link.desktop.schemes` (the bundler writes it into `Info.plist`) | Keep it equal to the address scheme, or links stop opening. Debug and release builds share it, like the identifier |
| Env vars `DATABEE_*` | `dev.rs`, `scripts/dev/launch.sh`, tests | |
| Window events `databee:*`, `window.databee` | `src/commands/index.ts`, `src/main.tsx`, the dev scripts | Internal only |
| Icons | `src-tauri/icons/` | Regenerate with `bun tauri icon <1024px png>` |
| Author, license | `Cargo.toml`, `package.json`, `LICENSE` | |

## Platform-specific code

The code is portable apart from the following. Nothing uses `cfg(target_os)` yet, and there are no direct
Objective-C or NSWindow calls. The macOS behavior comes from dependencies, paths and configuration.

| Piece | Where | Linux | Windows |
|---|---|---|---|
| **Keychain** (`security-framework`) | `secrets/keychain.rs` `Keychain`; the crate won't build elsewhere | Implement `SecretStore` with the `keyring` crate (Secret Service / libsecret) (see [swap the secret store](#swap-the-secret-store)), and gate `security-framework` under `[target.'cfg(target_os = "macos")'.dependencies]` | `keyring` (Credential Manager) |
| **Config dir** hard-coded to `~/Library/Application Support/<id>` | `lib.rs` `config_dir()`, `scripts/dev/js.sh` | `dirs::config_dir()` (`~/.config/<id>`). It must resolve before Tauri starts, because the CLI and socket run first | `dirs::config_dir()` (`%APPDATA%\<id>`) |
| **Unix socket** for the single-instance handoff (`std::os::unix`, `tokio::net::UnixListener`, mode 0600) | `ipc.rs` | Works as is. Mind the ~108-byte path limit; `$XDG_RUNTIME_DIR` is a good home | Doesn't compile. Use a named pipe (`tokio::net::windows::named_pipe`) with the same line protocol. Don't switch to `tauri-plugin-single-instance`: it passes argv, and DataBee keeps passwords out of argv on purpose |
| **File permissions** (`PermissionsExt`, `OpenOptionsExt`, `DirBuilderExt`; 0600 files, 0700 dir) | `ipc.rs`, `store/json_file.rs` | Works | Gate with `#[cfg(unix)]` |
| **Window chrome**: `titleBarStyle: Overlay`, `hiddenTitle`, `trafficLightPosition`, `transparent`, `windowEffects: ["sidebar"]`, `macOSPrivateApi` | `tauri.conf.json`; the Cargo feature `macos-private-api` | Ignored by Tauri. The CSS expects transparent chrome, so give `html`/`.app` an opaque `--bg-chrome` | `windowEffects: ["mica"]` works; otherwise use an opaque background |
| **Traffic-light space** and drag regions | `.toolbar` `padding-left` in `app.css`; `data-tauri-drag-region` in `Toolbar.tsx` and `ConnectScreen.tsx` | Remove the padding. With native decorations, drag regions are optional | Same; or set `decorations: false` and draw your own window controls |
| **Menu**: the app menu with Services, Hide and Show All | `menu.rs` | The menu bar renders inside the window. Drop the app menu and move Settings and Quit into File | Same |
| **Shortcuts**: `Mod` is ⌘, labels use ⌘⌥⇧ glyphs, `bindingFromEvent` assumes a Mac | `src/commands/registry.ts`; `metaKey` checks in `ResultsGrid`, `Sidebar`, `CommandPalette` and `FieldValue` | Map `Mod` to Ctrl, and render labels as `Ctrl+Shift+K` | Same |
| **Fonts**: bundled Geist and JetBrains Mono, with `-apple-system` and `Menlo` fallbacks | `fonts.css`, `theme.css` (`--font-*-bundled`) | Works; drop the macOS fallbacks or add `Cantarell` and `DejaVu Sans Mono` | Works; `Segoe UI`, `Cascadia Mono` as fallbacks |
| **Bundle targets**: only `app` | `tauri.conf.json` `bundle.targets`; `package.json` scripts open `bundle/macos/DataBee.app` | `deb`, `appimage`, `rpm` | `nsis`, `msi`; `icons/icon.ico` exists but `bundle.icon` doesn't list it; `main.rs` needs the `windows_subsystem` attribute so release builds open no console |
| **Install and dev scripts** (zsh, Swift, CoreGraphics, `screencapture`, `pkill`) | `scripts/` | Rewrite with `xdotool`/`wmctrl` plus `grim`/`import`, or drop `launch.sh` and use `bun tauri dev` | PowerShell |
| **Wording**: "Keychain", "Finder", the `~/Library/...` socket example | `db_commands.rs`, `error.rs`, `cli.rs` help, `components/settings/pages.tsx`, `ConnectScreen.tsx`, `Dialogs.tsx` | "keyring", "file manager" | "Credential Manager", "Explorer" |
| **Stage hook**: the presenter app is macOS-only | `stage/`, `src/stage/` | Remove ([Removing Stage](#removing-stage)) or leave off | Same |
| **Tests**: `navigator.platform = "MacIntel"` | `src/test-setup.ts`; tests that type `{Meta>}` | Make the platform a parameter, or drop it | Same |

A port is done when `bun run test`, `bun run lint` and `bun run tauri build` pass on the target, and the app
can do all of this:
1. connect over TCP and over a socket;
2. save and reuse a password in the OS secret store;
3. hand a second `databee …` launch to the running window;
4. show its menus and shortcuts with the right modifier.

## Common changes

| Goal | Start here |
|---|---|
| Add a database engine (SQLite, Postgres) | [database.md: adding another database engine](database.md#adding-another-database-engine) |
| Add rules for WordPress, a plugin or another application | [presets.md](presets.md); the built-ins are in [Built-in presets](presets.md#built-in-presets) |
| Add a command or shortcut | [frontend.md: commands](frontend.md#commands-shortcuts-palette-menu) |
| Add a menu item | [backend.md: native menu](backend.md#native-menu) |
| Add a setting | [backend.md: persistence](backend.md#persistence) |
| Add a Tauri command | [backend.md: Tauri commands](backend.md#tauri-commands) |
| Change colors, schemes, accents or row padding | [frontend.md: styling](frontend.md#styling) |
| Remove the Stage hook | [Removing Stage](#removing-stage) |

### Add a driver

`src-tauri/src/db/<engine>/mod.rs` implementing `Database`; an `Engine` variant with its `Dialect`; a `kind`
on `ConnectionSpec`; one arm in `db::open`. Nothing in `rules/` changes. Details:
[database.md](database.md#adding-another-database-engine).

### Swap the secret store

Add `src-tauri/src/secrets/<impl>.rs` implementing `SecretStore` (`get`, `set`, `delete`); `pub mod` it in
`secrets/mod.rs`; change the one `Arc::new(secrets::Keychain)` line in `lib.rs`; optionally delete
`keychain.rs` and `security-framework`.

### Removing Stage

Stage is a module on both sides ([stage.md](stage.md)). **Remove Stage**: `git rm -r src-tauri/src/stage src/stage src-tauri/capabilities/stage.json docs/stage.md`; delete `pub mod stage;` and `.plugin(stage::init(&config_dir))` in `lib.rs`; delete the `.plugin("stage", …)` line and `STAGE_COMMANDS` in `build.rs`; delete the `installStage` import and call in `src/modules.ts`; optionally strip `data-stage*` attributes; run `bun run lint && bun run test`.

### Removing the WordPress presets

WordPress, WooCommerce and Action Scheduler are presets, nothing else: delete `src-tauri/presets/wordpress.toml`,
`woocommerce.toml` and `action-scheduler.toml` and their lines in `rules::catalog::BUILT_IN`. The rules engine,
the readout, documents and the palette have no WordPress code, so nothing else changes. Optionally delete the
`wp_*` databases of `dev/fixture.sql` and the tests that read them (the preset tests listed in
[presets.md](presets.md#built-in-presets)), then run `bun run lint && bun run test`.

## Conventions to keep

- **Rust owns the logic.** The frontend doesn't build SQL.
- **IPC only in `api.ts` files.** Only the core's `src/api/index.ts` and a module's `src/<module>/api.ts` call Tauri.
- **Typed errors** cross the boundary as `{kind, message, code}`.
- **No secrets in files, logs or the webview.**
- **No silent writes.** Every write is planned, previewed, applied in a transaction, and undoable where
  possible.
- **Tests with every change**, through public interfaces ([`.claude/rules/testing.md`](../.claude/rules/testing.md)).
- **Small commits**, each one revertible ([`.claude/rules/commits.md`](../.claude/rules/commits.md)).
