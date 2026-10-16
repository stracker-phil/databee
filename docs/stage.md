# Stage hook (presentations)

Stage is a separate macOS presenter app. It drives other apps during a demo: it spotlights parts of a window,
shows captions, moves a drawn cursor, and clicks, types and runs commands. DataBee implements the Stage hook,
protocol 1, revision 2026-10-16, with every optional feature: `reveal visible sections ui type.append
keys.window scroll wait`.

The hook does nothing without a presenter. If you don't use Stage, you can ignore this page, or turn the hook
off under Settings → Presentations.

**Remove Stage**: `git rm -r src-tauri/src/stage src/stage src-tauri/capabilities/stage.json docs/stage.md`; delete `pub mod stage;` and `.plugin(stage::init(&config_dir))` in `lib.rs`; delete the `.plugin("stage", …)` line and `STAGE_COMMANDS` in `build.rs`; delete the `installStage` import and call in `src/modules.ts`; optionally strip `data-stage*` attributes; run `bun run lint && bun run test`. Both halves are modules, so nothing else refers to Stage.

## Transport

- **Server.** A local HTTP/1.1 server on `127.0.0.1:27184`, the default port, set under Settings →
  Presentations. Endpoints are `/v1/stage/{describe,window,act,state}`, taking GET or POST. Parameters come
  from the query string or a flat JSON or form body.
- **Rejected requests.** Requests with an `Origin` header or a foreign `Host` get 403, so browsers can't reach
  the server. The token is a bearer header or `?token=`, compared in constant time; a wrong or missing token
  gets 401.
- **Registration.** DataBee writes no files for the presenter. You register the hook with Stage once.
  Settings → Presentations shows the command with this app's URL and token filled in:
  `stage register http://127.0.0.1:27184/v1/stage --token <token>`. Stage reads the name and id from
  `describe`. The registration survives restarts of both apps. A new port or token needs a new registration.

```sh
URL="http://127.0.0.1:27184/v1/stage"; TOKEN="<token from Settings → Presentations>"
api() { curl -s -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" "$URL/$1" ${2:+-d "$2"}; echo; }
api describe
api window '{"window":"settings","action":"show","section":"presentations"}'
api act '{"kind":"command","command":"view:palette"}'
api act '{"kind":"type","anchor":"palette:search","text":"wp_opt","mode":"append"}'
api act '{"kind":"keys","keys":"Enter","window":"palette"}'
```

## Code

| Rust (`src-tauri/src/stage/`) | Responsibility |
|---|---|
| `protocol.rs` | Pure protocol types and helpers: key parsing (`split_modifiers`, `parse_last`, `parse_keys`) and canonical form, bounds, screen rects, the register command, `FEATURES` |
| `page.rs` | The messages between the hub and the page: `ActRequest` (a union on `kind`, sent as `ActEvent` with the request id; ts-rs `StageAct` and `StageActRequest`) and the replies `PageAnchor`, `AnchorsReply`, `ActReply`, `InfoReply`, `PageCommand`, `Surface` |
| `http.rs` | The HTTP server: `parse_head`, `check`, `params` and `response`, testable without sockets; size limits; Origin, Host and token checks (Host is `127.0.0.1` or `localhost` only; an IPv6 literal is refused); a 100 ms pause after a failed accept |
| `hub.rs` | `Hub`: request/reply correlation. It emits page work as Tauri events with a fresh request id and waits for the matching reply under `Timeouts`; a reply whose request timed out is dropped. `StageHost` abstracts the native window, so tests use a fake host. |
| `windows.rs` | The window model: `Frame`, `enum Window { Main, Settings, Palette }` (ts-rs `StageWindow`), `is_shown`, and the serialized `WindowInfo` and `AnchorInfo` in screen points |
| `requests.rs` | The verbs' requests, parsed before anything reaches the page: `ActKind`, `WindowRequest` (`window`, `WindowAction`, bounds, section), `type_payload`, `scroll_payload`, `wait_timeout` |
| `verbs.rs` | The verbs (`run`): `describe`, `state`, `window` (`main_window_action` or `sheet_action`) and `act` (by `ActKind`: anchor acts, `wait`, `keys`, `command`), keeping the protocol's status codes (400, 404, 409, 500) and messages. Answers are `Serialize` structs (`Describe`, `ActResult`). |
| `test_host.rs` | Tests only: `FakePage`, a host whose page answers like `hooks.ts`, and `hub_with` |
| `mod.rs` | `init(config_dir)`, the inline Tauri plugin ([ADR 0010](adr/0010-inline-tauri-plugins.md)): commands, managed `Stage`, start on setup, stop on exit. `Stage`: start, stop and restart from its settings, `update_settings`, `rotate_token`, status for the settings page (including the register command), last request time |
| `settings.rs` | `StageSettings` (enabled, port, token; `Debug` redacts the token) in `<config>/stage.json` through `StageStore` ([ADR 0016](adr/0016-module-settings-files.md)). `sanitized()` clamps the port and generates a token when it is empty (`new_token`: 32 random bytes as hex). `StageStore::open` takes the `stage` object of a legacy `settings.json` once, so a registered token survives the upgrade; it never writes `settings.json`. |
| `commands.rs` | The plugin's commands (`plugin:stage|…`): `stage_anchors`, `stage_done`, `stage_info` (the page's replies), `stage_status`, `stage_settings`, `stage_update_settings`, `stage_new_token`. `COMMANDS` must match `STAGE_COMMANDS` in `build.rs` (a test checks it); `capabilities/stage.json` grants them to the main window. |

The page side lives in `src/stage/`:

| File | Responsibility |
|---|---|
| `install.ts` | `installStage()`, called once from `installModules()` (`src/modules.ts`): registers the Presentations Settings page (`registerStageSettingsPage`) and starts the hooks (`installStageHooks`) |
| `api.ts` | `stageApi`, the module's only `invoke`/`listen` caller: `status`, `settings`, `updateSettings`, `newToken` (`plugin:stage|…`), the `stage:collect`, `stage:info` and `stage:act` subscriptions (`onCollect`, `onInfo`, `onAct`) and the replies `anchors` (the caller passes the page id), `done`, `info` |
| `apiMock.ts` | `createStageApiMock()` for tests that mock `./api` (status running, settings with `token-123`) |
| `types.gen.ts` | ts-rs output for the Stage types in `src-tauri/src/stage/` (`#[ts(export_to = "src/stage/types.gen.ts")]`); regenerate with `bun run types`, never edit |
| `types.ts` | Re-exports `types.gen.ts`, including `StageWindow` and `StageActRequest` (a union on `kind`, from `stage::page`) |
| `PresentationsPage.tsx` | Settings → Presentations: the switch, status, port, token and register command. It loads and saves `StageSettings` itself (`stageApi.settings`, `updateSettings`, `newToken`) and polls `stageApi.status` while open |
| `anchors.ts` | The anchors, read from the `data-stage*` attributes: `collectAnchors` (rects, visibility, role, list and states), `findAnchor`, `labelOf`, `roleOf`, `detailsOf`, `clipsOf`. `MAX_LABEL` (80) is the hook's only label limit; Rust passes labels through. |
| `actions.ts` | What an act does to the page: `reveal`, `click`, `typeInto` (replace or append; CodeMirror too), `scroll`, `press` (keys), `sheet` (open or close settings and the palette), `nextFrame` |
| `hooks.ts` | Dispatch only: answers `stage:collect` (`stage_anchors`), `stage:info` (`stage_info`) and `stage:act` with `perform`, an exhaustive `switch` on `kind` with one handler per kind, then `stage_done` |
| `state.ts` | `stageView()` (`connect`, `db`, `wp:<screen>`), `stageSurfaces()` (the settings window's sections are `settingsSections()`, from the settings page registry), `stageUi()` and the flat `stageState()`. `stageState()` never includes SQL text or passwords. |
| `commands.ts` | The registry's commands in Stage form, with shortcuts in Stage notation. Conventional ids: `app:settings`, `view:palette`. Families: `settings:open:<page>`, `table:open:<table>`, `wp:open:<entity>:<id>` |
| `rect.ts` | Rectangle math (`intersectRect`, `visiblePart`, `isInView`, `centerOf`) |

One verb, for example `act kind=click anchor=toolbar:settings`:
1. `http.rs` authenticates the request and parses it.
2. `verbs.rs` validates it into an `ActRequest`; `hub.rs` emits `stage:act {request, kind, …}` to the `main` window.
3. `hooks.ts` acts (`actions.ts`) and invokes `stage_done({request, ok, …})`.
4. The hub matches the request id and returns the JSON.

## Model

- **Windows.** `main` is the native window. Settings and the command palette are sheets inside it. The hook
  exposes them as the windows `settings` and `palette`, with `movable: false`; their bounds come from the page.
  `settings` lists its pages as `sections`. `main.view` reports the current view, and `main.ui.inspector`
  reports whether the inspector is open or closed.
- **Anchors** are `data-stage` attributes, with optional `-label`, `-role` and `-list`. Ids follow the data
  where rows have stable ids:
  - `table:<db>.<table>` (list `tables`)
  - `tab:table:<db>.<table>`
  - `<entity>:<id>` for WordPress rows (list `wp:<list>`)
  - `wp:nav:<list>`
  - `palette:search` and the palette results (list `palette:results`)
  - `settings:<page>` and `settings-nav:<page>`
  - `appearance:<control>` on Settings → Appearance (`theme`, `light-scheme`, `dark-scheme`, `row-padding`,
    `ui-size`, `data-size`, `font-ui`, `font-mono`); segment buttons add `:<option>`, accent swatches are
    `appearance:accent:<id>` (list `appearance:accent`)

  Sheets are marked with `data-stage-window`.
- **Commands** come from the in-app registry, with `shortcut` and `enabled`.
- **Keys.** When a key combination is a command's shortcut, that command runs in the current view, even
  without focus. So ⌘, ⌘K and ⌘T behave like their menu items. Other keys go to the focused element;
  `window=` focuses DataBee first. `hotkeys` is empty, because DataBee has no global shortcuts.

## Limits

- Synthetic clicks can't open native `<select>` menus: the connection and database pickers and the order
  status. Use the palette or commands instead.
- Tauri reports physical pixels, and the hook divides them by the window's scale factor. Coordinates are
  exact on one display, or on several displays that share a scale.
