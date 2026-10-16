# Stage hook (presentations)

Stage is a separate macOS presenter app. It drives other apps during a demo: it spotlights parts of a window,
shows captions, moves a drawn cursor, and clicks, types and runs commands. DataBee implements the Stage hook,
protocol 1, revision 2026-10-16, with every optional feature: `reveal visible sections ui type.append
keys.window scroll wait`.

The hook does nothing without a presenter. If you don't use Stage, you can ignore this page, or turn the hook
off under Settings → Presentations. To remove it from the code:
1. Delete `src-tauri/src/stage/`, `src-tauri/src/stage_commands.rs`, `src/stage/` and `src/lib/stage.ts`.
2. Remove their registrations in `lib.rs`, `App.tsx` and `src/api/`.
3. Remove `Settings.stage`.

The `data-stage*` attributes in the components are inert without the hook.

## Transport

- **Server.** A local HTTP/1.1 server on `127.0.0.1:27184`, the default port, set under Settings →
  Presentations. Endpoints are `/v1/stage/{describe,window,act,state}`, taking GET or POST. Parameters come
  from the query string or a flat JSON or form body.
- **Rejected requests.** Requests with an `Origin` header or a foreign `Host` get 403, so browsers can't reach
  the server. The token is a bearer header or `?token=`, compared in constant time; a wrong or missing token
  gets 401.
- **Discovery file.** While the server runs, it is published at
  `~/Library/Application Support/stage/apps/<bundle id>.json`. The file has mode 0600 and is written
  atomically. It holds the URL and the token and is removed when the hook stops or the app quits.

```sh
F="$HOME/Library/Application Support/stage/apps/com.philippstracker.databee.json"
URL=$(plutil -extract url raw "$F"); TOKEN=$(plutil -extract token raw "$F")
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
| `protocol.rs` | Pure protocol types and helpers: key parsing and canonical form, bounds, screen rects, discovery record, page reply types (`PageAnchor`, `ActReply`, `InfoReply`, `PageCommand`, `Surface`) |
| `http.rs` | The HTTP server: `parse_request` and `respond`, testable without sockets; size limits; Origin, Host and token checks |
| `hooks.rs` | The hub. It answers each verb, keeping the protocol's status codes (400, 404, 409, 500) and messages. It forwards page work as Tauri events and waits for the reply under a timeout. `StageHost` abstracts the native window, so tests use a fake host. |
| `discovery.rs` | Publishes and removes the discovery file |
| `mod.rs` | `Stage`: start, stop and restart from settings, status for the settings page, last request time |

The page side lives in `src/stage/`:

| File | Responsibility |
|---|---|
| `hooks.ts` | Answers `stage:collect`: anchors, read from the `data-stage*` attributes, with their rects, visibility, role, list and states. Answers `stage:act` with reveal, click, type (replace or append; CodeMirror too), keys, scroll and wait, then replies with `stage_anchors` or `stage_done`. |
| `state.ts` | `stageView()` (`connect`, `db`, `wp:<screen>`), `stageSurfaces()`, `stageUi()` and the flat `stageState()`. `stageState()` never includes SQL text or passwords. |
| `commands.ts` | The registry's commands in Stage form, with shortcuts in Stage notation. Conventional ids: `app:settings`, `view:palette`. Families: `settings:open:<page>`, `table:open:<table>`, `wp:open:<entity>:<id>` |
| `src/lib/stage.ts` | Rectangle math (`intersectRect`, `visiblePart`, `isInView`, `centerOf`) |

One verb, for example `act kind=click anchor=toolbar:settings`:
1. `http.rs` authenticates the request and parses it.
2. `hooks.rs` emits `stage:act {request, …}` to the `main` window.
3. The page acts and invokes `stage_done({request, ok, …})`.
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
