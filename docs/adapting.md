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
Remove the WordPress layer; I only need the plain SQL client.
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
| **Shortcuts**: `Mod` is ⌘, labels use ⌘⌥⇧ glyphs, `bindingFromEvent` assumes a Mac | `src/commands/registry.ts`; `metaKey` checks in `ResultsGrid`, `Sidebar`, `CommandPalette`, `FieldValue` and `WpWorkspace` | Map `Mod` to Ctrl, and render labels as `Ctrl+Shift+K` | Same |
| **Fonts**: `-apple-system`, SF Pro, SF Mono | `theme.css` | Add `system-ui`, `Cantarell` and `DejaVu Sans Mono` fallbacks | `Segoe UI`, `Cascadia Mono` |
| **Bundle targets**: only `app` | `tauri.conf.json` `bundle.targets`; `package.json` scripts open `bundle/macos/DataBee.app` | `deb`, `appimage`, `rpm` | `nsis`, `msi`; `icons/icon.ico` exists but `bundle.icon` doesn't list it; `main.rs` needs the `windows_subsystem` attribute so release builds open no console |
| **Install and dev scripts** (zsh, Swift, CoreGraphics, `screencapture`, `pkill`) | `scripts/` | Rewrite with `xdotool`/`wmctrl` plus `grim`/`import`, or drop `launch.sh` and use `bun tauri dev` | PowerShell |
| **Wording**: "Keychain", "Finder", the `~/Library/...` socket example | `db_commands.rs`, `error.rs`, `cli.rs` help, `SettingsDialog.tsx`, `ConnectScreen.tsx`, `Dialogs.tsx` | "keyring", "file manager" | "Credential Manager", "Explorer" |
| **Stage hook**: the presenter app is macOS-only | `stage/`, `src/stage/` | Remove or leave off | Remove or leave off |
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
| Add a WordPress entity, list, view or plugin | [wordpress.md: extending](wordpress.md#extending) |
| Add a command or shortcut | [frontend.md: commands](frontend.md#commands-shortcuts-palette-menu) |
| Add a menu item | [backend.md: native menu](backend.md#native-menu) |
| Add a setting | [backend.md: persistence](backend.md#persistence) |
| Add a Tauri command | [backend.md: Tauri commands](backend.md#tauri-commands) |
| Change colors, accents or density | [frontend.md: styling](frontend.md#styling) |
| Remove the Stage hook | [stage.md](stage.md) |

### Add a driver

`src-tauri/src/db/<engine>/mod.rs` implementing `Database`; an `Engine` variant with its `Dialect`; a `kind`
on `ConnectionSpec`; one arm in `db::open`. Nothing in `wp/` changes. Details:
[database.md](database.md#adding-another-database-engine).

### Swap the secret store

Add `src-tauri/src/secrets/<impl>.rs` implementing `SecretStore` (`get`, `set`, `delete`); `pub mod` it in
`secrets/mod.rs`; change the one `Arc::new(secrets::Keychain)` line in `lib.rs`; optionally delete
`keychain.rs` and `security-framework`.

### Removing the WordPress layer

WordPress mode is mostly additive, with one catch: the edit pipeline (`ChangeRequest`, `plan`,
`reinsert_sql`) lives in `wp/changes.rs`, and Database-mode grid edits use it too.

1. Move the generic variants (`SetCell`, `DeleteRow`, `DuplicateRow`, `InsertRow`) and their planning into
   `db/`, along with `plan_change` and `apply_changes`.
2. Delete `src-tauri/src/wp/`, `wp_commands.rs`, the `wp_*` registrations, `AppState.wp_cache` and the
   `wordpress` settings page.
3. In the frontend:
   - delete `src/components/wp/` and `src/lib/wpFormat.ts`;
   - delete the `wp`, `wpNav`, `mode` and `entityMeta` store slices;
   - delete the `go.*` commands, the `wp` and `document` scopes and the WordPress palette search;
   - remove the mode toggle in `Toolbar`, and render `DbWorkspace` directly in `App.tsx`.
4. Delete `tests/wp.rs`, `tests/php_roundtrip.rs` and the `wp_test`/`wp_write`/`wp_legacy` parts of the fixture.

Keep `wp/php.rs` and `wp/value.rs` if you want the Inspector to keep decoding PHP-serialized values.
Otherwise drop `decode_value`, and the Inspector falls back to JSON only, which needs a small decoder.

## Conventions to keep

- **Rust owns the logic.** The frontend doesn't build SQL.
- **One IPC boundary.** Only `src/api/index.ts` calls Tauri.
- **Typed errors** cross the boundary as `{kind, message, code}`.
- **No secrets in files, logs or the webview.**
- **No silent writes.** Every write is planned, previewed, applied in a transaction, and undoable where
  possible.
- **Tests with every change**, through public interfaces ([`.claude/rules/testing.md`](../.claude/rules/testing.md)).
- **Small commits**, each one revertible ([`.claude/rules/commits.md`](../.claude/rules/commits.md)).
