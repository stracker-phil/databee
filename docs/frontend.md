# Frontend

`src/` holds a React 19 + TypeScript app, bundled by Vite. State lives in one zustand store. The frontend
renders and forwards intent; [Rust owns the logic](architecture.md#design-rules).

## Boot

1. `index.html` loads `main.tsx`.
2. `main.tsx` calls `installModules()` (`src/modules.ts`, see [Modules](architecture.md#modules)) before the
   first render:
   1. `registerCoreSettingsPages()` (`components/settings/pages.tsx`).
   2. `installStage()` (`src/stage/install.ts`): the Presentations Settings page and the Stage event
      listeners, which stay subscribed for the app's lifetime.
   3. `installCommands()`: `registerCoreScopes()`, then the command registry. It runs last so module scopes
      exist before their commands.
3. `applyInitialScheme()` (`lib/theme.ts`) sets the default scheme for the OS appearance, so the first paint
   has colors before settings load.
4. `installNativeMenuGuard()` (`src/lib/nativeMenu.ts`): a right-click that no app menu handled shows
   nothing, so the WebView's own Reload and Inspect Element never appear; text fields and the SQL editor keep
   the native Cut/Copy/Paste menu.
5. `main.tsx` assigns the store to `window.databee` only when `import.meta.env.VITE_DEV_HOOKS` is `"1"`, which
   `bun run build:app` and `bun run app` set for the debug bundle the dev scripts drive; `bun run build`
   (release) leaves it out. It renders `<ErrorBoundary><App/></ErrorBoundary>`.
6. `App.tsx` imports the stylesheets (fonts, the scheme files, `theme.css`, `app.css`) and calls
   `useBackendEvents()` once:
   1. `loadSettings()` loads settings and keybindings. `installCommands()` runs again to apply the user's
      bindings, then `drainLaunches()` pulls the queued launch requests (`take_launch_events`) into
      `handleLaunchEvents` (`session` slice). `loadAccents()` loads the databases' accents.
   2. It subscribes to `launch` (drain again), `command` (menu clicks → `runMenuCommand`), `remote`
      (requests from `databee open` / `databee command` → `answerRemote` in `src/commands/remote.ts`, which
      always replies with `remoteReply`) and `session_health` (`setHealth`, see [Connection health](#connection-health)).
   3. It adds a capture-phase `keydown` → `handleKeydown`, `error` / `unhandledrejection` → error toast,
      `beforeunload` → `flushUiState`, and window `focus` → `retryLostSessions`.
7. `applyAppearance(settings, systemDark, accent ?? settings.accent)` and `syncRowHeight()` run whenever
   settings, the OS appearance or `selectEffectiveAccent` change.

Launch and remote "open" requests (`handleLaunchEvents`, `handle` in `commands/remote.ts`) go through
`openFromOutside` (`state/outside.ts`): an address, or an entity shorthand resolved with `api.document`, kept
with `keepDocument` and opened with `openObject`.

Layout:
- Without a session, the window shows `ConnectScreen`.
- With sessions, it shows `AppShell` (`Toolbar`, `Workspace`, `PendingTray`, `StatusBar`, `PeekOverlay`;
  `useValueHover` listens on it).
- `Dialogs`, `CommandPalette` and `Toasts` always render on top.

```
App
├─ ConnectScreen                       (no session)
└─ .app grid
   ├─ Toolbar                          connection / database text (pickers), ConnectionHealth, back/forward, search field (palette), icon buttons
   ├─ Workspace: [Sidebar |] main: TabBar + TabPanels + TableFocus
   │       TabPanels → Readout (header, needs attention, watched, recently created, pinned, recently opened; [Readout](#readout))
   │                 → QueryTab (QueryEditor, ResultsPane: ResultsGrid | ErrorPanel) [| value column]
   │                 → TableTab (filter bar + ResultsGrid | StructureView) [| value column]
   │                 → ObjectPanel: Trail, then the shown panes with Splitters between them (ObjectColumnView:
   │                                ObjectHeader, DocumentView | RowsView, or ValueColumnView)
   │                 → ProjectPanel (header, Overview: changes + problems | all rules: filter, a band per table; rule sheet)
   ├─ PendingTray
   ├─ StatusBar                        ▤ sidebar toggle, provenance or address, presets, pending n, live
   └─ PeekOverlay                      while ⌥ is held
Dialogs (ShortcutsDialog among them) · CommandPalette · Toasts
```

## Store

One zustand store with no middleware, composed from slices. `src/state/store.ts` is only the composition
root: it creates the store, keeps the slice registry and re-exports the selectors, `fail`, `flushUiState`
and the state types, so components import everything from `state/store`.

- `src/state/slices/*.ts`: one file per slice (`settings`, `session`, `tabs` (tab lifecycle), `query`
  (`runQuery`, `cancelQuery`), `changes`, `data`, `saved`, `ui` (toasts, dialogs, palette, hover, recent),
  `viewState` (the persisted per-connection `uiState` and its debounced writer), `nav`, `objects`, `columns`,
  `project`, `readout`, `live`, `health`). Each exports its
  member interface and a `Slice<T>`: `create(set, get)` returns the initial state and actions,
  the optional `sessionClosed(sessionId, state)` returns what to drop when a session closes.
- `Store` is an interface extending every core slice's interface. A module adds its members with
  `declare module "../state/store" { interface Store { … } }` and registers with `registerSlice(slice)`,
  which merges `slice.create(set, get)` into the live store, so registering after creation works.
- `resetStore(patch)` (tests) re-runs every registered slice's `create`, then applies `patch`.
- `src/state/selectors.ts`: the selectors, among them `selectSessionBySlug` (the session an address's
  `connection` names) and `noConnectionNamed(slug)`, the message when there is none; `src/state/requests.ts`: `fail`.
- Tab helpers in `src/state/slices/tabs.ts`: `findTab(s, tabId, kind?)` (the tab, narrowed to `kind` when
  given), `replaceTab(s, tab)` (the `tabs` update that puts `tab` in its place) and `hasValueColumn(tab)` (a
  table or query tab with a value column; re-exported from `state/store`) and `isPreviewTab(tab)`.
- Tab kinds: each `Tab["kind"]` has one entry in two typed records, so adding a kind fails to compile until
  both have one. `TAB_KINDS` (`src/state/tabKinds.ts`) holds what the store needs: `database(tab, s, column?)`
  and `preview` (the kind can sit in the session's preview tab: table, object). `tabDatabase(s, tab, column?)`
  is the database a tab works in, which the value commands and value columns use: a table tab's `database`,
  the row address of an object tab's active (or given) stack column, a project or readout tab's `database`,
  a query tab's session's current database. `TAB_VIEWS` (`src/components/tabViews.tsx`) holds how it looks:
  `icon`, `tooltip`, `stage` (its `data-stage` anchor), `Panel`, and the optional status-bar parts
  `addressHint` (a table's `· filtered`) and `Summary` (a query's results); it is separate because store
  modules must not import components. Behavior only some kinds have stays in a partial per-kind record next
  to it: change targets (`CHANGE_TARGETS`, `live.ts`). The Stage state line (`TAB_STATE`, `stage/state.ts`) is
  a full record too.
  Back/forward and `navigateTab` (`nav.ts`) act on the `DataTab` kinds only, turning one into the other.
- Slices import each other's reset helpers (`resetSelection`) and pure helpers (`activate`, `remembered`,
  `nextActiveSessionId`), never the store object, except where a module-level helper needs it (`fail`).

| Key | Meaning |
|---|---|
| `settings` | `Settings \| null`; null until loaded |
| `sessions`, `activeSessionId` | open connections (`SessionInfo`, with the connection `slug`) |
| `tabs`, `activeTabId` | `QueryTab` (sql, running, response, error, `valueColumn?`), `TableTab` (database, table, data/structure view, filter, structured `where`, preview, `valueColumn?`) `ObjectTab` (the column stack `columns`, the `active` index, the recorded `stacks`, `paneWidths`, `maximized`, preview; [Column stack](#column-stack); `DataTab` is a table or object tab, which `navigateTab` turns into each other) `ProjectTab` (database, the open rule `form`; [Project tabs](#project-tabs)) or `ReadoutTab` (database; one per session, [Readout](#readout)). Every tab carries `address` and `nav` ([Addresses and history](#addresses-and-history)) |
| `documents` | `document` replies by `session/<address or shorthand>`, tagged with their `dataVersion` (`loadDocument`, `keepDocument`, `selectDocument`) |
| `live`, `changeMarks` | live object columns by document key (`{ sessionId, address, id }`, `id` the Rust watch) and change marks by document key or `<session>/q:<tab id>` (a saved query's tab); see [Live mode, snapshots, compare](#live-mode-snapshots-compare) |
| `schema` | per session: databases and tables per database |
| `structures` | `describeTable` replies by `session/database/table`, tagged with the `dataVersion` they belong to (`loadStructure`, `selectStructure`) |
| `readouts` | `readout` replies by `session/database`, tagged with their `dataVersion` (`loadReadout`, `openReadout`; `watchValue`, `unwatchValue` and `labelWatched` change the [watched values](#readout) and read a loaded readout again) |
| `dialog` | one dialog at a time, as a discriminated union (settings, connect, password, confirm, prompt, insertRow, pin, shortcuts, snapshots, `action`: `{ address, action }` for the `ActionSheet`) |
| `pending`, `applied`, `applying` | plans Rust keeps (`PlannedChange`: id, description, statements for display, `canUndo`); the last 20 applied batch ids, for undo (`dismissUndo` marks the newest `dismissed`: `PendingTray`'s × hides its undo until the next apply, `selectOffersUndo`, while the `changes.undo` command still works); an apply or undo is in flight |
| `selectedCell` | `{ tabId, row, col, column, cell, source, value? }`, the selected grid cell of a tab (in an object tab: of its active column). `source` (`{ sessionId, resultId }`, `lib/cells.ts`) lets a value column and the value commands fetch a `longText` cell in full; `value` (set by grids) is the focused value of the status bar and the value commands |
| `hover` | `{ src, table, linked, sidebar }` under the pointer (`setHover`, from `useValueHover`): a value's provenance, or a table link (`linked`: a sidebar row, a section band); the status bar shows `src`, the linked table highlight reads `table` |
| `queryCounter` | query tabs opened per session, for unique default titles (`Query 3`) |
| `dataVersion` | bumped by ⌘R (`refreshView`, which first drops the session's schema cache with `refreshSchema`) and after apply or undo; views re-fetch when it changes |
| `historyVersion` | bumped when `runQuery` finishes; the history panel reloads on change |
| `trayFocusRequest` | bumped by `focusPendingTray()`; `PendingTray` focuses its Apply button (the status bar's `pending n` button uses it) |
| `uiState` | per connection identity: column widths, pane sizes (the SQL editor), `sidebarHidden` and `collapsedGroups` (`ui_state.json`); `toggleSidebar()` flips the active connection's flag, `toggleGroupCollapsed` an object group (`selectCollapsedGroups` reads them) |
| `savedQueries` | by `session/database`: the `SavedEntry` list of `saved_queries` (`loadSavedQueries`, read on every palette opening) |
| `health` | sessions whose connection is lost (`LostHealth`: Rust's `down` or `reconnecting` `Health`) by session id; a session that is not here is ok (`health` slice, [Connection health](#connection-health)) |
| `sidebarPanel` | `tables` or `history`, the sidebar's switch (`setSidebarPanel`) |
| `paletteSeed` | the text the palette opens with (`t:`); `CommandPalette` keys its sheet by it |
| `projectRules` | `api.projectRules` replies by `session/database`, per `dataVersion` (`loadProjectRules`; [Project tabs](#project-tabs)) |
| `rulesInfo` | `RulesInfo` by `session/database`: prefix, applied presets, entities, `tableEntities`, table icons ([Table icons](#table-icons)) |
| `coverage` | reference coverage counts by `session/database` (`CoverageCache`: `generation`, `results`; [Project tabs](#project-tabs)) |
| `accents` | each database's own accent by `accentKey(slug, database)` ([Styling](#styling)) |
| `paletteOpen`, `keybindings`, `toasts`, `recentNav` | as named |

Conventions:
- Components subscribe with narrow selectors (`useStore((s) => s.x)`), and `useShallow` for derived arrays.
- Event handlers call actions through `useStore.getState()`.
- Selectors live in `selectors.ts`, among them `selectActiveSession`, `selectActiveTab`, `selectCanNav`, `selectStructure`,
  `selectDocument`, `selectFocusedValue`, `selectCollapsedGroups`, `selectActivePending`, `selectLastBatch`,
  `selectCanUndo`, `selectSidebarHidden`, `selectMaxPanes`, `selectMaxCellChars`, `selectNullLabel` and
  `selectVisibleDatabases` (the active session's databases without system schemas unless shown or current; use
  with `useShallow`). `state/store` also re-exports slice selectors: `selectEffectiveAccent`,
  `selectDatabaseAccent` (`project`), `selectChangeTarget`, `selectActiveLive`, `selectCanDismissMarks` (`live`).
- Store actions wrap API errors in `fail(e, context)`, which shows an error toast and writes the message to
  the Rust log; a `connection_lost` error becomes a short `info` toast (`<context>: connection lost`)
  instead. Components that call `api` directly use the exported `fail` too, and update the UI only
  after the call succeeded (removing a recent connection, clearing history).
- Data loading: a component that reads from `api` outside the store uses
  `useApiResult(key, fetcher, { onError, keepData, delayMs })` (`lib/useApiResult.ts`), never a hand-written
  fetch in `useEffect`. The string `key` names every input of the fetcher (session, arguments and
  `dataVersion` when the view re-fetches on ⌘R); a new key fetches again, `null` fetches nothing, and a reply
  for a superseded key or after unmount is dropped. It returns `{ state: "loading" | "loaded" | "failed",
  data, error, reload }`. `keepData` keeps the last loaded data while another key loads or after a failure
  (pages, history, completion); `delayMs` debounces (palette row hits, the coverage preview). Error policy:
  a component with a place for the error shows `error` there (`ErrorPanel`, a form error, the peek card,
  the value counts popover, the history panel, the Stage status pill); one without passes
  `onError: (e) => fail(e, context)` for a toast (decoding a value, palette row hits, recent connections,
  Stage settings). Only a best-effort enhancement logs instead (`api.logError`: the SQL editor's completion
  schema). No failure is swallowed. `value/useDecoded.ts` is the one decoder hook of `Value` and the value
  column.
- Clipboard: `copyText(text, what)` (`lib/clipboard.ts`) is the only clipboard writer. It toasts
  "Copied {what}" or that the clipboard is not available.
- Formatting (`lib/format.ts`): `formatCount` for every count shown in full (`12,345`), `compactCount`
  for tight spots (`1.2k`, `3.4M`), `relativeTime(isoOrUnixSeconds)` for past and future times
  ("2 h ago", "in 3 d").
- Overlays: `Modal` portals into `document.body` (so no scroll box's stacking context or type reaches it) and
  draws one of three `variant`s: `sheet` (default; `.dialog.sheet`, the padded form or message), `panel`
  (`.dialog.panel`, the overlay surface laid out by its content: the palette, rule sheets, Settings) or `bare`
  (the connect dialog, whose `.connect-card` is its own surface). It traps Tab inside the sheet, focuses the first control unless a child has `autoFocus`,
  and returns focus to the opener on close unless the closing action focused something else. It handles
  Escape in the capture phase and stops it there, so only the sheet closes (keys inside `[data-recording]`
  pass through). The palette renders inside `Modal` too. Every context menu is `ContextMenu.tsx`: callers build
  `MenuEntry[]` (items `MenuItem` `{ label, action, disabled?, title?, detail? }`, `detail` a second line in
  `.menu-detail`, used by `actionMenu`; and `"separator"`) and call `open(anchor, entries)`
  from `useContextMenu()`, rendering its `element`; the anchor is a pointer event (its default menu is
  suppressed) or a point. The menu portals into `document.body`, so it never inherits the font of the grid or
  document it opened from, and `.menu` in `app.css` sets the UI font itself. It focuses its first item, moves
  with ArrowUp/Down, closes on Escape or an outside click, and positions itself from its measured size.
- Writes: `queueChange` asks Rust to plan a row edit (`planChange`), keeps the returned plan with the table it
  writes (`target`) and resolves to its id (null when refused); `queueActionPreview(sessionId, id)` queues the plan of a rule action's preview (`queueActionPreview`); `applyChanges` sends the active session's plan ids, `undoLast` the newest batch id
  (`undoBatch`); `discardChange` / `discardAll` drop plans in Rust too. The page never sends SQL to write.
- In-flight guard: `applyChanges` and `undoLast` return early while `applying` is set; they set it before
  the first `await` and clear it in `finally`. `PendingTray` disables Apply and Undo meanwhile. `runQuery`
  refuses a tab that is already `running`.
- Selection: `selectCell` tags the cell with the active tab. `clearSelectionFor(state, tabId)` drops it when
  its tab closes or is left (`activate`); a `dataVersion` bump (`bumpData`) clears it outright.
- `disconnect(sessionId)` closes the session in Rust, then applies one `set` that merges every registered
  slice's `sessionClosed` result: the session and the next active session (`session`), its schema and
  `structures` keys (`session`), its tabs, `queryCounter` and the selection if its tab was closed (`tabs`),
  its `pending` and `applied` entries (`changes`), its `documents` (`objects`), its `savedQueries` (`saved`), its `projectRules`, `rulesInfo` and `coverage` (`project`),
  its `readouts` (`readout`), its `live` columns and `changeMarks` (`live`) and its `health` entry (`health`). A slice that keys data by session implements `sessionClosed`.
- `switchDatabase` resets the selection and moves the session's readout tab, if it has one, to the new database
  without activating it. `connect` resets the selection and opens the readout of the session's database ([Readout](#readout)), no
  other tab.
- Tab updates are typed: `updateQueryTab` and `updateTableTab` only patch tabs of their kind.
- View state (`viewState` slice: `setTableWidths`, `setPaneSize`, `toggleSidebar`, `toggleGroupCollapsed`) is written debounced; `flushUiState` writes it on
  `beforeunload` (registered in `App.tsx`).
- Some errors become state instead: a password dialog, a tab's `ErrorPanel`, a confirm dialog for
  `confirmation_required`.

## API boundary

Only `api.ts` files import `@tauri-apps/api` or `@tauri-apps/plugin-*`: the core's is `src/api/index.ts`, a
module's is `src/<module>/api.ts` (Stage: `src/stage/api.ts`) ([ADR 0002](adr/0002-ipc-in-api-files.md)). `src/api/index.ts` exports `api`, a flat object of thin `invoke` wrappers with no logic whose members include
the event subscriptions (`api.onLaunch`, `api.onCommand`, `api.onRemote`, `api.onSessionHealth`), plus
`toAppError` and the types. The IPC types are generated from Rust with `ts-rs`
([ADR 0014](adr/0014-generated-ipc-types.md)) into `src/api/types.gen.ts` (a module's types into its own folder, e.g. `src/stage/types.gen.ts`); never edit them. `bun run types`
regenerates them (it runs the `export_bindings_*` tests), and `bun run lint` fails when a committed file is
stale. `src/api/types.ts` re-exports the generated types plus the few frontend-only ones (`Keybindings`, `PaneSize`, `EMPTY_UI_STATE`, `TEXT_SIZES`, `SETTINGS_LIMITS`, `LOG_LEVELS`,
`ACCENTS`, `SCHEMES`, `BUNDLED_FONTS`). `ACCENTS` and `SCHEMES` mirror Rust's lists of the same name. `TEXT_SIZES` and `SETTINGS_LIMITS` mirror the clamps in Rust's `Settings::sanitized`, which stays the
source of truth; change both together. Command signatures in
`api` stay hand-written. `toAppError(e)` normalizes rejections to `AppError` and accepts only a known `kind`. Derived connection facts come from Rust and are never recomputed in the page: the
Keychain `account` (which also scopes history) is on `SessionInfo`, `RecentConnection` and login
errors (`AppError.account`).

## Commands, shortcuts, palette, menu

Every user action that has a shortcut or appears in the palette is a `Command` (`src/commands/registry.ts`):

```ts
{ id, title, scope, defaultBinding?, when?, run, menu?, fallbackFor? }
```

The core commands are `APP_COMMANDS` (`src/commands/index.ts`); their default bindings are listed in
[ux.md](ux.md#keys).

- **Bindings** are strings like `Mod+Shift+K`, where `Mod` means ⌘. They are normalized to the order
  `Mod, Ctrl, Alt, Shift` and shown as `⌘⇧K`. A trailing `++` is the `+` key (`Mod+Shift++`).
- **Scopes** are registered (`src/commands/scopes.ts`) and form a tree through their `parent`:
  ```ts
  interface ScopeDef { id, parent, label, isActive() }
  registerScope(def)          // replaces a scope with the same id; throws on an unknown parent
  activeScopes()              // every registered scope whose isActive() is true
  isAncestor(a, scope), specificity(scope), scopeLabel(id)
  ```
  The core registers `global` → `db` → `table` / `editor` and `global` → `object` (`registerCoreScopes()`,
  called by `installCommands`); a module registers its scopes before `installCommands` runs. Registering a
  command with an unregistered scope throws, which catches typos. When several commands match a key, the
  most specific active scope wins; specificity is the depth in the parent chain (`global` is 0). The
  `label` names the scope in Settings → Shortcuts.
- **Keys.** `handleKeydown` runs in the capture phase, so app shortcuts beat CodeMirror and inputs. Inside text
  inputs, only `Mod`/`Ctrl` combinations and Escape fire. While a dialog or the palette is open, only the
  commands in `MODAL_ALLOWED` (Settings toggle, the shortcut overview, zoom) run; other keys reach the overlay. Native menu items
  go through `runMenuCommand`, which applies the same rule. Auto-repeated keydowns are swallowed unless the
  command is in `REPEATABLE` (zoom, back/forward, next/previous tab, the focus moves of the column stack). Keys inside a `[data-recording]`
  element (the Settings → Shortcuts recorder) are left alone, so already-bound combos can be recorded.
- **Key names.** `bindingFromEvent` uses `e.key`, so punctuation follows the user's layout. With ⌥ held,
  for dead keys and for non-Latin characters it uses the physical key from `e.code` instead (`KeyK` → `K`,
  `Digit1` → `1`, `Minus` → `-`), so ⌥⌘K records as `Mod+Alt+K`.
- **Overrides** come from `keybindings.json` (`null` unbinds). Settings → Shortcuts records new bindings and
  flags conflicts in overlapping scopes.
- **Palette** (`CommandPalette.tsx`, `components/palette/`) is the main navigation. Its sections, in order:
  Recent, row hits (an `Address` item for `databee://…`, then Rust's sections), Saved queries (`q:`), Connections and Databases (`c:`), Tables, Commands (those
  whose `when()` passes, plus the active object's rule actions, `objectActionCommands()`).
  - `parsePrefix` (`palette/parse.ts`) reads `>` (commands), `t:` (tables), `q:` (saved queries, see
    [Saved queries](#saved-queries)), `c:` (switch connection), `#`, `@`, `=` and `databee://` (the whole text is the needle). `searchesRows` says when Rust is asked:
    `#<digits>`, `@`/`=` with 2+ characters, free text with 6+ (`MIN_TEXT`); shorter text matches only tables
    and commands (substring `score`).
  - `useRowHits` calls `api.paletteSearch` 200 ms after the last keystroke; a newer input drops older
    replies, and a failed search toasts. While the session's connection is lost it searches nothing and
    `PaletteFooter` says `connection lost: rows are not searched`. The hits come in Rust's sections, titled by what they are (`objects with id 4711`, `rows that
    reference 4711`, `people matching jane`, `external id …`, `<table>.<column>` for `=`; see
    [architecture](architecture.md#palette-search)).
  - `PaletteList` renders a hit as its `source` (table, or `table.column`) in `--o2`, `#` and its key in the
    accent, then its detail `Value`s; other items as title, badge and key cap. A section's label is an
    `li role="presentation"` (`.palette-section`) before its first option, so options keep their indices and
    names. It scrolls the active item into view. `PaletteFooter` lists the keys and the prefixes, and for `#` without entities says that it reads
    only tables under 1M estimated rows.
  - Enter on a hit (`openHit`) drills into the active object tab's stack (`drill` after its active column),
    else opens an object tab; ⌘↵ (`goAlt`) always opens a tab. A hit whose address is not a row opens with
    `openAddress`. Tables open in the preview tab, pinned with ⌘↵. Holding ⌥ peeks the selected hit
    (`usePeekHeld`, `palette/usePeekHeld.ts`; `PeekCard` under its row). `openAddress` of a `?find=<value>` address opens the palette with `=<value>`,
    and so does "Find everywhere" on a value.
  - `c:` (`connection.switch`, ⌘⇧P) is the connection switcher, `useSwitchItems` (`palette/useSwitchItems.ts`):
    Connections (open sessions, the active one first, then `api.listRecents` minus the open identities) and
    Databases (`selectVisibleDatabases` of the active session), scored with `fuzzyScore`. An item whose database
    has its own accent (`accents`, see [Styling](#styling)) shows an `AccentDot` before its title; an open
    session whose connection is lost has `· connection lost` in its badge (`current · connection lost`). Enter runs
    `setActiveSession`, `connect` (its password dialog included) or `switchDatabase`. `CommandPalette` keys the
    sheet by `paletteSeed`, so a palette command that reopens it with a prefix starts fresh.
- **Menu.** Native menu items emit `command` events with a command id; `runMenuCommand` runs that command, or
  the one whose `menu` names it, whose scope is active and whose `when()` passes (`databee command` uses it
  too). The movement keys of the column stack (J/K/H/L, arrows, ↵, ⎋) have no menu item. See
  [backend.md](backend.md#native-menu) for the Rust side of `menu`.

Some targets live inside a component, such as running the editor's statement or focusing the WHERE field.
Those commands dispatch window events (`databee:run-query`, `databee:focus-where`,
`databee:focus-table-filter`), and the active component listens for them.

Adding a command:

```ts
// src/commands/index.ts, in APP_COMMANDS
{
  id: "tabs.duplicate",
  title: "Duplicate query tab",
  scope: "editor",
  defaultBinding: "Mod+Shift+D",
  when: () => selectActiveTab(useStore.getState())?.kind === "query",
  run: () => {
    const tab = selectActiveTab(useStore.getState());
    if (tab?.kind === "query") useStore.getState().openQueryTab(tab.sql);
  },
},
```

The command then appears in the palette, in Settings → Shortcuts and in the Stage command list. Test it with
`installCommands(); runCommand("tabs.duplicate")`, or by sending its keys to a rendered `<App/>`.

## Components

| Area | Components |
|---|---|
| Shell | `Toolbar` (with `ConnectionHealth`, see [Connection health](#connection-health)), `StatusBar`, `PendingTray`, `Splitter` (resizable panes, persisted per connection; the column stack's pane dividers; also `usePaneSize`, the SQL editor's persisted height), `useWindowSize.ts` (`useWindowSize`, the window size for the SQL editor's maximum height), `ErrorBoundary`, `Field` (`InsetField`: the macOS field frame with icon and key cap; `KeyCap`), `LongList` (see [Rendering](#rendering)) |
| Connect | `ConnectScreen` / `ConnectForm`: TCP or socket, recent connections |
| Database workspace | `Sidebar` (tables tree with filter and arrow-key navigation over a roving `tabIndex`, history reloaded on `historyVersion`; table rows of the current database carry `data-table` and take `lit`; a cut-off name shows whole on hover through `OverflowReveal.tsx` `useOverflowReveal(rowSelector, labelSelector)`, a portaled overlay in the row's font, color and opaque background), `Tabs`, `tabViews.tsx` (`TAB_VIEWS`; a table or query tab inside `WithValueColumn`, which adds its value column on the right without moving the tab in the tree), `QueryTab`, `QueryEditor` (CodeMirror, MySQL dialect, schema completion), `ResultsGrid` (virtualized, resizable, sortable, keyboard navigation, copy as TSV/JSON, inline edit, a column header menu from `columnMenu`; `grid/`: `useColumnSizing` (measured, saved and dragged widths, auto-fit), `useCellEditing` (the inline editor, full text of `longText` cells) and the pure `cellMenu` (the cell context menu); its other hooks and `fitWidth` in `gridHooks.ts`), `ValueCounts` (the value counts popover, see [Value counts](#value-counts)), `TableTab` (paged browse, smart or SQL filter, column filter chip; the Structure view in `structure/`, see [Structure view](#structure-view)), `ErrorPanel` (query and load errors), `ValueTree` (PHP/JSON decode) |
| Values | `value/Value.tsx` (`Value`: one stored value drawn by its kind), `value/useValueHover.ts` (the status bar's hover listener), `value/useOpenValue.tsx` and `value/open.ts` (what a click or Enter on a value opens, `ColumnContext`), `value/ValueColumnView.tsx` (a value column), `value/Peek.tsx` (`PeekOverlay`), `value/useAltHeld.ts` (`useAltHeld`: ⌥ held alone, one shared set of window listeners), see [Values](#values) |
| Objects | `object/`: `ObjectPanel` (the stack, pane dividers), `Trail`, `PaneMaximize` (`PaneContext`), `useWidth`, `ObjectColumnView`, `ObjectHeader`, `DocumentView`, `RowsView`, `GroupHeader`, `AlsoLinked`, `groups`, `fields`, `actions` (`actionMenu`, `objectActionCommands`), `ChangeList`, `CompareView`, `WideTable` (a row table that scrolls sideways in its own box; `pinned` keeps a primary-key first column in view; used by `groups` and `ChangeList`), `useDocument`, `useViewScroll`, `useCollapsedGroups`, see [Object tabs](#object-tabs) and [Column stack](#column-stack) |
| Project | `project/`: `ProjectPanel`, `ProjectHeader`, `Overview`, `TableSection`, `ReferenceLines`, `RuleSections`, `ReferenceForm`, `KindStyleForms`, `coverage`, `formParts`, see [Project tabs](#project-tabs) |
| Overlays | `Modal`, `Dialogs` (the insert dialog sets each column to a value, NULL or its default; default leaves it out of the INSERT; the pin dialog picks Not pinned, Count, Value or Rows with a row count), `SettingsDialog` (see [Settings pages](#settings-pages)), `ShortcutsDialog` (`?`: the bound commands of the scopes active when it opened, most specific first, from the registry), `CommandPalette` (with `palette/`: `parse`, `usePaletteItems`, `useRowHits`, `useSwitchItems`, `PaletteList`), `ContextMenu`, `SnapshotsDialog`, `Toasts` |
| Readout | `readout/Readout.tsx`, see [Readout](#readout) |

### Saved queries

`saved_queries` lists, for the active session's database, the
project's queries (`origin: { kind: "project" }`), the global ones (`global`, from migrated favorites, offered in
every project) and the `[[pinned]]` queries of the applied presets (`{ kind: "preset", preset }`, `readOnly`;
`savedOriginText` shows `preset:<name>`), `{p}` and `{np}`
replaced. The `saved` slice keeps them by `session/database`.

- `q:` in the palette lists them (`savedItems` in `usePaletteItems.ts`, scored by title, then id): the title,
  the origin in `--o2` (`Item.note`), `pinned` as the badge when the query has a pin. Enter runs
  `openSavedQuery`: a query tab with the SQL, run at once. A project or global query that is already open is
  activated instead; a preset query opens as an unsaved tab titled `copy of <title>`.
- A query tab remembers where it came from (`QueryTab.saved: SavedRef { id, origin, database, pin }`). The
  action bar shows the origin, **Save** (`query.save`, declared `fallbackFor: "changes.apply"`: it takes ⌘S, the binding of
  `changes.apply`, and runs while nothing is pending; the conflict check skips such a pair) and **Pin** (`query.pin`, `p` while the focus is in the results grid). Save
  updates a project or global query in place (`update_saved_query`), else asks for a title and stores a new
  project query (`save_query`). Pin opens the pin dialog and stores `pin` (`{ show: "count" }`, `{ show: "value" }` or
  `{ show: "rows", limit }`, the shape of `[[pinned]] home`; `lib/pins.ts` maps it to the dialog's choices and the
  button label); an unsaved or preset tab is told to save first.
- On a preset query's tab, **Copy to project** (`copyPresetQuery`, `copy_preset_query_to_project`) stores the
  preset query as a project query; the tab then saves to that copy.
- `databee://<connection>/<database>/q/<id>` opens the saved query with that id (`openAddress`).

### Value counts

Every grid column header has a menu (right-click, `ResultsGrid`'s `columnMenu`) with **Value counts**: a `--pop`
popover (`ValueCounts`) listing the column's most frequent stored values (`value_counts`, top 50) with their
counts. Each value renders with `Value`; a click (or Enter) on its row filters:

- in a table tab, it adds `column = value` to the tab's structured filter (`filterRows`; NULL is `IS NULL`),
  and the counts stay inside the tab's current structured filter;
- in a query tab, the counts are of the column's origin table (`ColumnMeta.origin`), and a click asks Rust to
  add `column = value` to the tab's SQL (`narrow_query`), then runs it. The menu item is disabled, with the
  reason as its tooltip, when the column is not read from a table or when the SQL is not a single plain
  single-table `SELECT` (`narrow_refusal`).

Binary and long text values show but cannot filter.

### Settings pages

`src/registry/settingsPages.ts` holds the pages; `components/settings/SettingsDialog.tsx` only renders them.

```ts
interface SettingsPage { id, group, label, icon, intro?, Body, order? }
registerSettingsPage(page)   // replaces a page with the same id
settingsPages()              // by group (Interface, Database, System, then others by name), order, label
settingsGroups()             // the same, grouped for the nav
```

- `Body` gets `{ s, set }`: the current `Settings` and a patch function that saves quietly. A page with its
  own settings file (Stage's Presentations page) ignores them and loads its own.
- The dialog shows the page `dialog.page` names, or the first page when it is absent or unknown. It
  re-renders on registration (`useSettingsPagesVersion`), so a page may register after the dialog opened.
- The Stage hook lists `settingsPages()` as the settings window's sections and the `settings:open:<page>`
  choices, so a registered page is reachable from Stage without further code.
- `components/settings/`: `controls.tsx` (`Group`, `Row`, `Switch`, `Segmented`, `TextField`,
  `NumberField`), `ShortcutsPage.tsx`, `pages.tsx` (`registerCoreSettingsPages()`: Appearance, Shortcuts,
  Queries, Display, Connections, Logging). Numeric bounds come from `SETTINGS_LIMITS` /
  `TEXT_SIZES`, the log levels from `LOG_LEVELS`.

Adding a page from a module: write the component next to the module's code (it may import the shared
controls), then call `registerSettingsPage` from the module's install function:

```ts
registerSettingsPage({ id: "presentations", group: "System", label: "Presentations", icon: Presentation, order: 10, intro: "…", Body: PresentationsPage });
```

### Workspace layout

`App.tsx`: `Main` returns the connect screen or the workspace (`AppShell` wraps the toolbar, pending tray,
status bar and `PeekOverlay`). In `Workspace`, `main` (`TabBar`,
`TabPanels`, `TableFocus`) keeps the same tree position whatever the sidebar does: a hidden sidebar (`view.sidebar`,
`selectSidebarHidden`) leaves an empty slot and adds `no-sidebar`, so CodeMirror history and table pages
survive. The sidebar width is the `--sidebar-width` token in `theme.css`. There are no inset panels: chrome
(`--chrome`) and content (`--bg`) meet at `--line` hairlines. Values open as columns, never in a docked pane.

- **Toolbar:** `ConnectionText` shows `connection / database` as mono text; a transparent native select over
  each name (`Connection` only with several sessions, `Database` always) opens the picker. A database with its
  own accent gets an `AccentDot` before its name (aria-hidden; the title names the accent). `NavButtons` (back
  and forward, `toolbar:back`/`toolbar:forward`) show for a table or object tab, run `nav.back`/`nav.forward`
  through `runCommand`, are disabled while `selectCanNav` finds no history and name the bound key in their
  title. Nothing between them and the search field: an object tab's place is its trail (see
  [Column stack](#column-stack)), a table tab's table is its title. `SearchField` is a
  button styled as a field (label "Go to") that opens the palette; its key cap follows `palette.open`.
- **Status bar:** `SidebarToggle` (▤, `aria-pressed` while the sidebar shows), `Address` (the provenance of
  the hovered or selected value while there is one, see [Values](#values); otherwise the active tab's address as text, a button that copies it, with `· filtered` when a
  typed SQL filter applies; a query tab adds rows, time and statement count, with the row-limit warning), spacer, `PresetsText` (`presets wordpress · woocommerce`, the
  applied presets of the active database; a click opens its Project page), `PendingCount` (`pending n`, a button
  to the tray when n > 0), `LiveText` (`● live 2 s` while the active column is live; a click ends it). There is
  no theme control: the theme stays in Settings and the palette.
- **Sidebar:** the Tables/History switch, the filter (`InsetField`, key cap from `sidebar.focusFilter`), a
  `db · N tables` caption, then the tree. Rows are `.si` items (`sel` for the active table). All tabs of the active session
stay mounted; inactive panels are `hidden`. Tabs use a roving `tabIndex` with Left/Right arrows
(`TabItem`; icon, tooltip and anchor from `TAB_VIEWS`). Shortcut hints (new tab, run statement, run all, editor placeholder) come from
`useBindingLabel(commandId)` in `commands/index.ts`, so they follow rebinding.

### Values

`components/value/Value.tsx` renders every stored value: `Value({ value, onOpen?, density? })`, a `memo`
component, drawn per kind as [ux.md](ux.md#value-grammar) lists. `density` is `grid` (one line, cut to
`maxCellChars`; serialized data followed by its raw text) or `full` (the default; serialized data expands in
place to a `ValueTree`, decoded with `api.decodeValue` on first expand). `onOpen(value, event)` fires for
the kinds that act on click (`opensOnClick`: a primary key with a row address, a reference with a target that is
not known to be missing, an enum, serialized data in a grid); the caller decides what that does, usually through
`useOpenValue(owner)` (functions in `value/open.ts`): a primary key opens its row as an object (the key of the
object `owner` shows switches that object between Document and Rows instead), a reference opens its target
(several: a menu), serialized data opens a value column (`showValue`), an enum runs `value.filter`. Inside an
object tab's column (`ColumnContext`, `{ tabId, index }`) a target or another row opens as the next column
(`drill`). `clickEffect(value, density, owner, place)` says what a click does: `drill` (a column opens: the next
column of the stack for a row target or another row's key, a value column for serialized data in a grid), `go`
(the whole view moves or a tab opens: `openTarget` moves a table tab in place with `navigateTab`, elsewhere the
preview tab) or `null` (an enum filters, the own row's key toggles the view). The handlers use the same
predicates (`targetEffect` for reference targets, the own-row test of `openRowObject`). The `openValue` that
`useOpenValue` returns carries `effect(value, density)` (`OpenValue` in `Value.tsx`), and `Value` passes it to
the value's leading icon. A primary key with a row address and a reference whose targets are in one table
start with that table's icon (`leadTable`, `LeadIcon`: `<TableIcon effect>`; [Table icons](#table-icons)),
serialized data in a grid with `▸` (`.ser-mark`). With an effect, `OpenSwap` (`value/OpenHint.tsx`) holds the
icon and the effect's glyph (ChevronRight for `drill`, ExternalLink for `go`, `.open-glyph`) in one
`.open-swap` box of 1em (`aria-hidden`, `data-open=<effect>`, no pointer events, no handlers); CSS hides the
glyph and, while the nearest `.grid-cell`, `.row` child (one without a `.summary-pair`), `.summary-pair` or the
parent link is hovered, or the selected cell of a focused grid, hides the icon and shows the glyph instead. Both
are rendered, so a hover re-renders nothing; the box keeps its width, so nothing shifts or is cut. Without an
effect (the own row's key, no open handler) the icon stands alone. A reference with targets in several tables
has no leading icon and shows `OpenHint` (an inline `.open-hint`, same `data-open`) after its text, visible on
the same hover; links outside values without a table icon use it too (a readout value's title, a pinned title,
"Show rows"). `LEAD_ICON_CH` (2.5ch) is the width a leading icon adds, `hasLeadIcon(value)` says whether a value
has one; `ResultsGrid` passes them as `leads` to `useColumnSizing` (`fitWidth(…, lead)`), `groups.tsx` to
`charWidth(…, LEAD_ICON_CH)`. A secret (`SecretText`) reveals on click, and while ⌥ is held over it (`useAltHeld`). A style sets `color: var(--style-<name>)`. CSS classes are `val`
and `val-<kind>` (`styles/app.css`, values).

Every value carries `data-src` (`provenanceText`) and `data-table` (its provenance table). `useValueHover`
(`AppShell` in `App.tsx`) adds one delegated `mouseover`/`mouseleave` listener to the app, reads the nearest
`[data-src]`, `[data-table]` or `[data-row-table]` at most once per animation frame and writes `hover`
(`setHover` skips equal values): a `.val` element is a value (`linked: false`), anything else with `data-table`
a table link (`linked: true`, and `sidebar` inside `.sidebar`; a sidebar row, a section band, whose `data-src` is the rule's origin). A row of a
known table carries `data-row-table` and counts as a value of that table (`linked: false`) wherever the pointer
is on no value: Document rows (`groups.tsx`, the group's table; events rows, the value's), grid rows
(`ResultsGrid`, when every sourced value of the row shares one table), readout rows and palette hits
(`PaletteList`, which runs its own `useValueHover` since the palette is outside `AppShell`). The status
bar's address slot shows `hover.src`, else the provenance of `selectedCell.value`, else the address.
Interactive values with a provenance are Stage anchors `value:<table>.<column>:<key>` (list `values`).
A reference with one target also carries `data-ref` (the target address), `data-via` (its column) and `data-by`
(the rule, `bySourceText`) for the peek.

**Peek** (`value/Peek.tsx`, `PeekOverlay` in `AppShell`): window listeners in the capture phase. A keydown of ⌥
alone (no dialog or palette open) peeks the hovered `[data-ref]` element, else the focused value
(`selectFocusedValue`) when it is a reference with one target; moving the pointer onto another reference while
⌥ is held peeks that one; keyup or a window blur hides it. The card sits below the element (fixed, kept in the
window), reads `api.peek(sessionId, address)` (`Peek { address, keyColumn, summary, matches }`), cached by
session, `dataVersion` and address, and draws `summary` as a two-column table (`SummaryTable`: one row per
value, name and `Value`; the card grows to its content between 320px and 560px, longer values end in an
ellipsis). For a set (`matches`) it heads the
card with the table and its filter and lists the match rows (`#key` + `SummaryLine`, buttons that open the row;
the card is `pointer-events: none` except for them). `PeekTarget.from` is the row the reference is stored in
(`data-from` on the value, `rowIdentity` in `lib/provenance.ts`): a match with that identity is dropped and the
total lowered, and a set that resolved to that very row reads "no other row matches". ↵ while it shows opens
the target with `openTarget` (the next column when the reference is in a stack column).

**Summary line** (`value/SummaryLine.tsx`): renders a `Summary` (Rust's one row-summary rule) as
`name value` pairs, the name tertiary and the value as `Value` in grid density (NULL as the null label). The
object header, the peek and the readout's timeline and recently opened rows all use it; palette hits keep their
unnamed `detail` values.

**Value commands** (`commands/values.ts`: `VALUE_COMMANDS`, ids `value.*`, scope `table`, and
`OBJECT_VALUE_COMMANDS`, ids `object.*`, scope `object`) act on the focused value, `selectFocusedValue`: the
selected grid cell of the active tab, else the active column of an object tab: the focused value of its Document view, or a value column's value (none while its row is `missing`). They are rebindable
like every command.
Bare keys never fire in inputs or the editor (`resolveKeydown`).

| Command | Key | Does |
|---|---|---|
| `value.copyRaw` | `y` | copies the stored text (a `longText` cell fetched in full; NULL as empty) |
| `value.copyCondition` | `Y` | `api.valueCondition(sessionId, database, provenance, raw)`: Rust renders the condition from `ColumnEquals` with the session's `Dialect`; on the value column of a `[[meta]]` table it reads the row's key and returns `` `meta_key` = '…' AND `meta_value` = '…' `` |
| `value.edit` | `e` | dispatches `EDIT_VALUE_EVENT`; the grid of the active tab that holds the selection, or the active Document view, opens its inline editor there, which queues a core `setCell` on the provenance row through the pending tray (documents: rows with a single-column key, meta rows by their own id) |
| `value.filter` | `f` | `filterByValue`: in a table tab of the value's own table, `filterRows(tabId, { column, value })` (nav slice): adds the match to the table tab's structured filter, replacing one on the same column, through `showRows` (so ⌘[ returns); otherwise it opens the value's table filtered by it in the preview tab |
| `value.addReference` | `r` | `openRuleForm("reference")`: `api.ruleDraft(sessionId, database, provenance)` (the column as rules name it, the meta key on a meta value), then `openProject` with a prefilled `ReferenceForm` |
| `value.setKind`, `value.setStyle` | | the same with a `KindForm` or a `StyleForm` (the style takes the value's text) |
| `value.watch` | | watches the value's cell on the readout of the tab's database (`watchValue(source)`, the `readout` slice's `watchValue`); binary cells too, since the readout reads the cell itself |

Binary cells offer none of `Y`, `e`, `f` and the rule commands, since their cell holds only a preview. The grid's
cell menu and the Document view's value menu (right-click) list `WATCH_MENU` ("Watch on readout", for any
sourced value) and `RULE_MENU`: "Add reference…", "Kind…", "Style…".

`ValueTree` takes `onWatch(path)`: a right-click on a node (a line, or one pair of an inline line) opens a menu
with "Watch on readout" for that node's keys from the root (the tree's `labels`; the root line is the whole
cell) and stops there, so the value's own menu does not open too. `Value` passes it for a sourced value's
inline tree and `ValueColumnView` for its tree, both through `watchValue(source, path)` (`commands/values.ts`:
the active tab's session and `tabDatabase`).

### Table icons

`components/TableIcon.tsx` maps the curated icon names (the `TableIcon` type, generated from the Rust enum
`rules::icons::TableIcon`) to lucide components in `TABLE_ICONS` (static imports; `Record<TableIcon,
LucideIcon>`, so tsc fails when the two lists differ; `table` is lucide's `Table2`) and `ICON_NAMES` lists them
for the picker. `useTableIcon(database, table, sessionId?)` reads `rulesInfo[projectKey(session,
database)].tableIcons[table]` (session: the given one, else the active one), a string, so one selector per
component is cheap; until the database's `rulesInfo` is loaded, or for a table it lacks, it returns
`DEFAULT_ICON` (`table`), so the glyph is there from the first render and only changes shape; `null` only
without a table. When `rulesInfo` is missing it calls `ensureRulesInfo` (`project` slice), which loads it once
however many components ask. `<TableIcon database table sessionId? effect? fallback? />` renders `IconGlyph`
(`.tbl-icon`, `1em`, `--o2`, `aria-hidden`, `data-icon=<name>`), inside `OpenSwap` with an `effect`
([Values](#values)), or `fallback` without a table. It is placed before table names: `Sidebar` rows, table and
object tabs (`tabViews.tsx`; an object tab whose root has no table falls back to the generic icon),
`GroupHeader` and `ObjectHeader` (table buttons, `effect="go"`; the plain table name outside a stack has none),
its "Also matched" (`go`), palette table items and hits (`PaletteList`, no effect: the palette is chosen with
↵), the readout (status values' tables; entity queries' table buttons and the `#key` links of recently created
and opened rows, `go`), `TableSection`; and in `Value` before a reference or a row's key (`LeadIcon`).

`project/IconControl.tsx` shows a table's `IconRow` (`[icon] menu · preset wordpress: *meta`, `· edited in
project, was …`; without a rule `[icon] table (default)`) and "icon…", which opens `IconPicker` in a `Modal`
(`rule-sheet icon-sheet`): a filter field ("Filter icons", focused; it keeps the names that contain the typed
text, ↵ picks the first), a grid of the matching `ICON_NAMES` (the shown one pressed, `table` without a rule;
"No icon matches" when none does), "Reset to preset" or "Remove <pattern>" for a project rule (`removeIcon` of
its pattern), Cancel. A choice stores `setIcon { rule: { table: <rule
name>, icon } }` through `ProjectContext.change` (`changeProject`), so it bumps `dataVersion` and reloads
`rulesInfo` like any rule edit. Without a rule name (a table outside the prefixes) there is no picker.

### Results grid

`ResultsGrid` resets its selection and inline editor when `rows` changes identity and clamps indices
before use. It renders the header itself and each virtual row as `GridRow` (`grid/GridRow.tsx`, cells as `GridCell`). It has no row-number column. Header cells show the column name with the sort arrow, and the type
below it. Hover is plain CSS (`--row-hover` on the row, `--cell` on the cell), so crossing cells never
re-renders the grid. Cells render `Value` with `density="grid"`, from the `valueAt(row, col)` prop
(`TableTab`: `lib/values.ts` `valueAt` on the page; `QueryTab`: `resultValueAt`; without it `cellValue`);
values are built once per row and rows identity, so `Value`'s memo holds while the grid re-renders. The
only kind class on a cell is `num` (right alignment for number cells). Clicks go through `useOpenValue(owner)`:
a primary key opens its row as an object in the preview tab, a reference opens its target in place in a table
tab (`navigateTab`) and with `openAddress(…, { preview: true })` in a query tab (several targets: a menu), an enum runs `value.filter`, serialized data
opens a value column (in an object tab's column: the next column of the stack, through `ColumnContext`). On a
primary key cell, double-click opens the object pinned and Enter in the preview tab (in a stack: the next
column); Enter on a reference with one target opens it, on serialized data or long text a value column with the
full text; otherwise double-click and Enter edit. Keys live in `KEY_ACTIONS` (`grid/gridKeys.ts`: arrows, Home/End,
PageUp/PageDown, Enter, ⌘C); the cell and header menus in `grid/useGridMenus.ts`. `owner` names the object whose rows a grid shows (Rows view), `focus` selects and
reveals a cell (the focus carried over from the Document view). `bytes`
cells are never editable: they show a `0x…` preview, which an edit would write back. `changes` (a table tab's
live check, `RowChange` by row index) marks rows (`db-changed`, `db-removed`) and the cells whose stored
value differs (`db-changed`, `db-flash` within `FLASH_MS` of being seen, `data-db-change`); ⌥ held over such a
cell shows `ChangePeek` (`grid/ChangePeek.tsx`, portaled to `body`): the shown value → the stored one.
`value/Peek.tsx` peeks no reference while ⌥ is over a `data-db-change` cell. `onEditingChange` reports the
inline editor opening and closing.

**Pinned key column:** when the first column holds primary keys, the grid is `grid pinned`: that column stays in
view while the grid scrolls sideways, and `revealColumn` keeps the selected cell right of it. Keyboard moves
reveal rows with `revealRow`, which keeps the selected row between the sticky header (`.grid-head`, measured)
and the bottom edge. `markScrolled`
(`gridHooks.ts`) sets `data-scrolled` on the scrolled box, which draws the pinned column's edge. Document row
groups do the same with `WideTable pinned` (`keyFirst` in `groups.tsx`). The pinned cells paint `--pin-bg`, which
`TableFocus` sets for lit rows.

Rust cuts text longer than the `maxCellChars` setting to a `longText { len, preview }` cell
([database.md](database.md#cell-decoding)); the grid shows the preview with its length in characters. The
grid's `source` prop (`{ sessionId, resultId }`) names the result set: editing or copying a `longText` cell
fetches its full value with `fullCellText` (`api.fetchCell`) first, never the preview, and without a
`source` the cell is not editable. A value column gets the full text the same way before showing or decoding it.

`QueryTab`'s results pane shows a partial script result (`ExecuteResponse.failure`) as an `ErrorPanel`
titled "Statement N failed" above the results of the statements before it. A `timeout` error with
`sessionReset` adds an info toast that temporary tables, variables and open transactions are gone.

### Table tabs

`TableTab.tsx`: `useTableStructure` reads the store's `structures` cache (`loadStructure`, keyed by
`session/database/table`, one `describeTable` per `dataVersion`, purged on disconnect) for `DataView`.
`DataView` browses only once the structure (or its error) is known, so an ID
filter always has the primary key. `useBrowse` drops replies that are not the latest request, and the
`Pager` moves by `BrowsePage.limit` (Rust's clamped page size). A failed structure shows an alert and
leaves editing off. `FilterBar` and `useRowEditing` hold the filter input and the row menu and cell edits;
primary-key and id columns match case-insensitively. The typed filter and `where` live on the
tab, not in `useBrowse` (which keeps only offset and sort), so back and forward can change them; other
rows start at the first page. Applying the filter calls `setTableFilter` (no history entry: a typed
filter is not addressable). The tab's `filterMode` (smart when unset; `openTableTab` with a filter, as the
readout and the project page open rows, sets `sql`) goes into the request; the field's mode button and
`table.filterMode` (⌃⌥X) call `toggleFilterMode`, which runs an applied filter again in the other mode. An `ids` or
`text` filter shows a chip with its label that toggles to the SQL it ran. The tab's `where` goes into every browse request next to the typed filter;
`ColumnsFilterChip` shows `BrowsePage.columnsFilter.label` read-only (the SQL as its tooltip), and its clear
button calls `showRows` with no matches, which records the filtered address for back. The Reload button
(`table:reload`) fetches the page again and calls `reloadValueColumn(tab, null)`, so a value column opened from the
grid reads its value again too ([Column stack](#column-stack)).

**Live check** (`TableCheck.tsx`, pure state in `lib/tableCheck.ts`): `useTableCheck(tab, page, { hasPrimaryKey,
editing, loading })` reads the checksums of the page's rows (`api.tableCheck`) right after each page loads (its
result id: Refresh, paging, a filter, ⌘R or an applied write start over), as the baseline, then every
`TABLE_CHECK_MS` (4 s) while the tab is the active tab, the window has the focus and is visible, no page is
loading, the grid's inline editor is closed (`onEditingChange`) and no pending plan writes the table (a pending
item's `target`; a rule action's plan has none and counts as writing any table). A tab in the background, a
filter or a sort never pause it: rows are read by key. `withHashes` marks a row `changed` (another checksum) or
`removed` (none) and unmarks one back at its baseline; `rowsToRead` picks the changed rows not read at their
current checksum yet, which one `api.tableCheckRows` reads; `withRows` keeps their stored cells and changed
columns. One check (`checkPage`) runs at a time; a reply for an earlier page is dropped. A failed check stops checking for
that page. Settings → Queries "Live check in table tabs" (`tableLiveCheck`, default on) turns it off. Nothing it
learns changes the page: `TableChangesBar` counts the marked rows above the grid (`changesText`) with a
Refresh button (the Reload button's action), `TableCheckHint` says in the table head why a tab does not check
(`no primary key`, a key that is not text, pending changes, a failed check with its message as tooltip), and
the grid draws the marks (`changes`, [Results grid](#results-grid)); the bar adds "⌥ on a marked value shows
old → new" while rows changed. The check also pauses while the connection is lost (`pausedBy = "connection
lost"`, hint "live check paused · connection lost"); `checkStatus` turns the pause reason (`PauseReason`) and
the page into the `TableCheckStatus` the hint draws, and editing pauses without a hint.

### Structure view

`structure/StructureView.tsx` renders `api.tableRules(sessionId, database, table)` (a `TableProfile`, loaded
per `dataVersion` with `useApiResult`, the last one kept while the next loads;
[presets.md](presets.md#the-structure-view)). The page only renders: Rust resolves which rules apply to which
column. One grid row per column (`role="row"`, Stage anchor `structure:column:<name>`): name, type (`columnFacts`:
`nullable`/`not null`, `default …`, extras, comment in `--o3`), key marks (`PK`, `UNI`/`idx` with the index name and the column's
position in it), then the kind, reference and value-style cells, each a stack of rule items. Below:
a "meta keys" block per `MetaKeyRules`, "Referenced by", "Entities", "Actions" (`writesSummary` from
`object/actions.ts`) and the DDL. Above the columns, the table's `IconControl` (`TableProfile.icon`;
[Table icons](#table-icons)).

- **Rule items** (`structure/RuleItems.tsx`): `ReferenceItem`, `KindItem`, `StyleItem`, each a mono main line
  and a meta line with the origin (`ruleOriginText`, a glob kind or style adds its pattern), coverage
  (`CoverageCell`), the `RuleSwitch` and `ActionButtons` from `project/`. They decide the controls like the
  Project page: from the row's `id`, `origin` and `edited`; foreign keys are read-only; a kind that is not the
  first for its column reads `overridden`. `ReferenceText` keeps the two ends' conditions apart (`when this row has …` on the row's side,
  `(rows with …)` on the other end; `other <table> rows with the same <column>` for a column that references itself). Reference
  ends are buttons that open that table's Structure view.
- **Coverage:** `useAutoCoverage` over every applied reference of the profile, so counts share the Project
  page's per-database cache.
- **Forms:** the view keeps its own `form` state and renders the Project page's `ReferenceForm`, `KindForm`
  and `StyleForm` with a `ProjectContext` built by `useRuleControls`: the store's `projectRules` (loaded with
  `loadProjectRules`, for `tables`) and a `ProjectTab`-shaped object from the table tab. Until the project's
  rules load, the controls are hidden and the "Add …" items disabled.
- **Column menu** (`useContextMenu`, on right-click or the row's `⋯` button): "Add reference…" (a reference
  form from the column), "Add value style…" and "Add kind…" (scoped to `<ruleName>.<column>`;
  `structure/forms.ts`), "Open rows" (the Data view).

### Opening rows

The page never builds SQL conditions. Opening rows in the Database view passes structured column matches
(`ColumnEquals`: `{ column, value }`, a `null` value means `IS NULL`), and Rust renders and quotes them
([Browsing tables](database.md#browsing-tables)):

- Value clicks go through `useOpenValue` and `filterRows`; `showRows(tabId, { database, table, where })` moves
  a table tab to other rows and records the earlier ones for ⌘[.
- `openTableTab(database, table, view?, filter?, { preview?, where? })`: an open tab is reused only when
  its `filter` and `where` are the same. A preview tab that replaces the previous one takes over its history
  and records the replaced address, so a single click in the sidebar can be undone with ⌘[.

### Object tabs

An `ObjectTab` shows rows as a stack of columns (`components/object/`; see
[Column stack](#column-stack)). Each object column (`ObjectColumn`, `lib/columns.ts`) has its row `address`,
whose `view` mirrors the column's `view` (`null` for the document, `rows`), and `focus`, the provenance of its
focused value. The tab's `address` is the active column's. `openObject(address, { preview })` (`objects` slice)
reuses the tab whose first column is that row (and makes that column active) and otherwise opens one, in the
preview tab when asked: a preview of either kind (table or object) replaces the session's
previous preview and takes over its history (`previewTab`, `placed`). Rows open as objects from a primary key
click, from a grid's double-click or Enter on a primary key cell, from `openAddress` with a row address, from
the table tab's row menu ("Open as object") and from `openEntity(entity, id)` (`objects` slice: the entity
shorthand `order 4711` through `document`, kept with `keepDocument`, opened at its canonical row address).
Every `openObject` records the row as the newest recent object of its project (`api.rememberObject`), which
the [readout](#readout) lists.

- **Loading:** each column (`ObjectColumnView`) calls `useDocument(sessionId, text, database)`, which reads
  `documents` and calls `loadDocument` once per `dataVersion`, so ⌘R and applied writes refresh it; the previous
  document stays while reloading. The key is the address without its view (`documentText`), so toggling never
  refetches. A failed load renders `ErrorPanel`. The tab title becomes `<entity> #<key>` of the first column once
  its document names its entity.
- **`ObjectHeader`:** table name, `#key`, the Document/Rows segmented control (`ViewToggle`) with the `object.viewDocument` /
  `object.viewRows` key caps (in a pane beside the active one, `QuietPaneContext` true, a `.view-label` text naming
  the view instead), the actions button, the summary (`Document.header`) as a `SummaryLine`, other rows an entity
  shorthand matched (`alternatives`). The `Actions` button (only when the row has actions) and a right-click
  open the actions menu (`actionMenu`), each item with what it writes as its `detail` line (`writesSummary`). The
  right-click of a header with a `place` appends `paneMenu()` (`commands/columns.ts`: "Open in new tab", "Close
  pane", those whose command applies to the active pane) after a separator.
  An action opens the `ActionSheet` dialog (`object/ActionSheet.tsx`, dialog type `action`): a field per param
  (choices as a datalist, free text allowed), and the `previewAction` result, fetched again 250 ms after a param
  changes: per row `table #key` and per column `old → new` as `Value`s, deleted rows, or the statements of free
  SQL; then the row count, undo, and that the change is queued for review in the pending tray. "Queue change"
  queues that preview (`queueActionPreview`). The palette lists the same actions.
- **`DocumentView`:** per group a `GroupHeader` band (`data-table`, relation text from `relationText`, count,
  the rule's origin as `data-src`; the table name opens the group's rows in a table tab), then the body:
  `FieldsGroup` and `MetaGroup` as `.row.kv` rows, `RowsGroup` as a `.row.hdr` header plus rows on a grid
  template sized in characters, `EventsGroup` as time, table and column or key. Values render through the
  memoized `Field` (no effects per value); a mousedown focuses its provenance (`focusObject`), the focused
  value gets an accent outline on `--sel`, and stays in view as `j`/`k` move it. `+n keys` / `+n rows` under a
  capped group switches to Rows and scrolls to that group. Zero-row groups collapse into `AlsoLinked` ("Also linked: …").
- **`RowsView`:** per group with a table its band, the `WHERE` of its rows address, and a compact `ResultsGrid`
  (at most its header, 12 rows and a sideways scrollbar tall (`.rows-grid` `max-height` from `--row-h`,
  `--grid-head-h`, the tallest header, and `--scrollbar-size`), then it scrolls): a browse of the group's `rowsAddress` with the table's full columns and
  editable cells, or, for a group no address names (patterns), its document rows read-only.
- **Toggle:** `object.viewDocument` / `object.viewRows` (`⌘1` / `⌘2`, scope `object`; `Tab` stays focus
  navigation) and the segmented control call `toggleObjectView(tabId, index?)` / `setObjectView(tabId, view, focus?, index?)` on the
  active column (or the column of the control), which push the address being left onto the tab's history.
  Focus follows the provenance: Document → Rows selects the same cell in its group's grid, Rows → Document
  takes the selected cell's provenance as `focus`. Both views of a column share its `.object` scroll container
  (`overflow-anchor: none`), so `useViewScroll` keeps each view's own offset per column: a view starts at the
  top the first time, gets its offset back later (re-applied while its grids still load), then reveals the
  focused value. A column that is hidden and shown again starts at the top.
- **Collapse:** every `GroupHeader` has a chevron button (`aria-expanded`, "Collapse/Expand <table>"); it and a
  click on the band outside its buttons call `toggleGroupCollapsed(identity, database, "<document table>::<group id>")`
  (`viewState` slice, `slices/viewState.ts`), stored in `UiState.collapsedGroups` by database (`ui_state.json`, debounced), so the choice holds
  for every row of the document's table. `useCollapsedGroups(owner)` reads it for both views. A collapsed group
  keeps its band and drops its body and `+n` button; in Rows its grid does not mount, so nothing is browsed. A
  click or key in the view ends `useViewScroll`'s offset restore, so collapsing does not jump back.
- **Linked table highlight:** `TableFocus` (`TableFocus.tsx`, inside `main` in `Workspace`) renders a `<style>`
  while `hover` is a table link (`linked`), so no component subscribes to the hover. In `.main` it lights that
  table: its rows (`[data-row-table]`) as one `--lit-solid` band, pinned key cells included (`--pin-bg`); its
  bands (`.sh[data-table]`) `--lit-solid` with an inset accent bar and the name in the accent; other links to it
  in the accent. References into it from other rows (`.grid-cell:has(> .val[data-lead-table])`, else the
  `.val[data-lead-table]` itself; `leadTable` in `Value.tsx`) get `--cell`, so they look hovered. With
  `hover.sidebar` (the link is a sidebar row) the cells of other rows (other tables' rows, readout lines and
  anomalies), other bands and entity chips drop to `--dimmed`, except cells holding a reference or link into
  the table. Sidebar
  rows take `lit` for a link or a hovered value of that table: the name in the accent, the icon unchanged, no
  background; only the selected row (`sel`) has one.

### Live mode, snapshots, compare

Rust diffs; the page keeps the marks and draws them ([backend](backend.md#watches-and-snapshots), look in
[ux.md](ux.md#live-mode-snapshots-compare)).

- **`live` slice** (`state/slices/live.ts`): `selectChangeTarget(s)` is what the commands act on: the active
  object column (key `documentKey(session, documentText(address))`) or a query tab of a saved query (key
  `queryMarksKey(session, tab)`, address `…/q/<id>`). `toggleLive()` (`w`, `object.live`) starts a watch
  (`api.watchStart`) or stops it; while any column is live, one timer (`setTimeout`, Settings
  `liveIntervalSecs`) runs `pollLive()`, which stops the watches of documents no object tab shows any more and
  polls the others (`api.watchPoll`, one request per watch at a time), keeps a changed document with
  `keepDocument` and merges the delta into `changeMarks`. A failed poll (an ended watch, a lost connection)
  stops live mode for that column with an error toast. `dismissMarks()` (`x`, `object.dismissChanges`,
  `query.dismissChanges`) drops the target's marks; live mode stays on. `takeSnapshot()` (`s`,
  `object.snapshot`, `query.snapshot`) prompts for a label and calls `api.snapshotTake`; `chooseSnapshot()`
  (`object.sinceSnapshot`, `query.sinceSnapshot`) opens the `snapshots` dialog (`SnapshotsDialog.tsx`: pick,
  delete), and `showSnapshotDiff(target, id)` replaces the target's marks with those against the snapshot.
  `selectActiveLive` feeds the status bar's `● live 2 s` (`statusbar:live`, a click stops it).
- **Marks** (`lib/diff.ts`, pure): `ChangeMarks { since, changed, added, removed, lines }`; `withDelta(marks,
  delta, at)` adds one poll (a row that comes back leaves `removed`, a removed row drops its value marks);
  `documentMarks(marks)` and `compareMarks(delta, side)` give the `DocumentMarks` a `DocumentView` draws
  (`diff` prop), `withRemovedRows(groups, removed)` puts removed rows back into their groups.
- **Drawing** (`object/groups.tsx`, `fields.tsx`): a row with a changed or new value gets `chg`, a removed row
  `removed`; a `Field` with a change seen in the last `FLASH_MS` (1.6 s) wraps its value in `.flash` (keyed by
  the change time, so a new change flashes again) and shows the old value in `.old` (`no row` for a new
  value). `ChangeList` (`object/ChangeList.tsx`) renders the `changes` band and its lines under the column
  (and under a saved query's results in `QueryTab`); a line focuses its value in the Document view
  (`setObjectView(…, "document", source)`), in a query tab it opens the row as an object.
- **Compare:** `objectColumn(address)` keeps `compare` on a document column, so the tab's address and history
  carry `?compare=<key>`. `ObjectColumnView` renders `CompareView` for it: `api.compare` once per
  `dataVersion`, both documents side by side with `compareMarks`, the left one kept with `keepDocument` so the
  value commands act on it. `object.compare` ("Compare with another row…") prompts for the key.

### Column stack

`ObjectTab.columns` (`StackColumn[]`, `lib/columns.ts`) holds `ObjectColumn`s and `ValueColumn`s
(`{ value, text, database, version?, queued?, missing? }`: a stored value and its full text, after a queued edit
the edited text; the `dataVersion` the text was read at, the plan ids its edits queued, and whether its row was
gone when it was read again); `active`
is the column `h`/`l` move, the trail marks and the value commands act on; `paneWidths` and `maximized` are the
tab's pane layout (never stored). The `columns` slice
(`slices/columns.ts`) changes it:

- `drill(tabId, from, target)`: `target` as an object column after column `from`, closing any to its right,
  active. Clicks and Enter reach it through `ColumnContext` (`value/open.ts`), which each column provides.
- `openValueColumn(tabId, from, column)`: the same for a value column; for a table or query tab (`from` null)
  it sets the tab's `valueColumn`, which `WithValueColumn` (`tabViews.tsx`) renders right of the tab. Both
  stamp the column with the current `dataVersion`. `updateValueColumn(tabId, index, text, planId)` keeps an
  edited text and the plan that queued it.
- `reloadValueColumn(tabId, index)` (index null: a table or query tab's value column) reads the value again with
  `fetchStoredValue(sessionId, database, provenance)` (`lib/cells.ts`: a one-row `browseTable` whose `where` is the
  provenance's row key, then `fetchCell` for a `longText` cell), so it finds the row whatever the grid's order.
  It writes `text` and `value.raw`, or `missing` when no row has the key, and the `dataVersion` it started at;
  while a plan in `queued` is still pending it only stamps the version, so the edited text stays. A value without
  provenance (query results) is left alone. A failed read stamps the version, keeps the text and toasts "Could
not reload the value". The write is dropped when the tab no longer holds that column. Its
  callers: `ValueColumnView` (`useCurrentValue`) whenever the column's `version` is older than `dataVersion`
  (⌘R, applied writes, undo, a pane that mounts again after one), and the table tab's Reload button.
- `popToColumn(tabId, index)` (the close button of a drilled column), `closeRightmostColumn()` (`Esc`,
  `columns.close`; a table or query tab's value column first, then a maximized stack is restored before any
  column closes), `setActiveColumn(tabId, index)` (a mousedown in a column, in the capture phase so the column
  is active before a grid selects; a trail crumb), `moveActiveColumn(delta)` (`h`/`l`), `closeActiveColumn()`
  (`column.close`: `popToColumn(tab, active - 1)`, so the columns drilled from the active one close with it; the
  first column never closes). A new active column drops the tab's grid selection.
- `setPaneWidths(tabId, widths)`: the shares of the shown panes (summing to 1; null: equal).
  `toggleMaximized(tabId)`: only the active column shows. A stack down to one column drops `maximized`
  (`withTab`); drilling, popping and back/forward keep it (`objectified` carries the layout of an object tab),
  so the new active column shows maximized.

`ObjectPanel` is the trail and the stack: it measures the stack's width (`useWidth`, a `ResizeObserver`) and
`shownColumns(count, active, width, maxPanes)` (`lib/columns.ts`) picks the panes, `[start, end)`: at most
`maxPanes` (`selectMaxPanes`, the `maxPanes` setting), as many as fit at `MIN_PANE_WIDTH` (560px) with
`DIVIDER_WIDTH` between them, one always, ending at the newest unless the active column lies further left. A
maximized tab shows only the active column. Only the shown columns mount; a hidden one has no DOM. Each pane is
a `section` (`aria-current` when active; while several show, class `active` with a 2px accent line on top, the
others class `quiet`, which mutes their accent and background, see [ux.md](ux.md#layout)) with `flex-grow` from
`paneShares(tab.paneWidths, panes)` and, beside others, `min-width: MIN_PANE_WIDTH`. Shares of another number
of panes are cleared (`useResetShares`), except while maximized. The active column takes the keyboard focus
(`tabIndex=-1`) when it or the shown panes change and nothing outside the panel holds it (`useFocusActive`).

`PaneDivider` is a `Splitter` between two panes (`role="separator"`, vertical, `aria-valuenow` the left pane's
width in px, ← / → in 16px steps, "Resize <left> and <right>"): a drag sets the two panes' `flex-grow` directly
and commits the shares with `setPaneWidths` on release, clamped so both keep `MIN_PANE_WIDTH`; a double-click
(`onReset`) makes them equal. `Splitter` takes an optional `stage` anchor and `onReset`.

`PaneMaximize` (`PaneMaximize.tsx`) is the toggle in a pane's header ("Maximize pane" / "Restore panes",
`aria-pressed`, `Maximize2` / `Minimize2`): `ObjectHeader` renders it when it has a `place`, `ValueColumnView`
always; it reads `PaneContext`, which each pane provides while `maximizable(tab, maxPanes)` holds (several
columns and `maxPanes` > 1, or maximized), so it is absent in table and query tabs. It makes its pane active,
then calls `toggleMaximized`.

**Trail** (`Trail.tsx`, `nav` "Trail" above the stack): the root row's table ("Open table <table>", which calls
`navigateTab(tab.id, wholeTable(root address))`), then one crumb per column, the name from the column's
document entity, else the table's only entity in `rulesInfo.tableEntities`, else the table name (`crumbName`), a
value column's column name; `aria-current="location"` on the active one. The crumbs of the shown panes are
wrapped in `.trail-panes`. Every column crumb calls `setActiveColumn`, so a hidden column shows without closing
any. `trailUnits(count, start, end, table, steps)` (`lib/trail.ts`) folds hidden crumbs into "…" buttons
(`MoreCrumb` for a `TrailMore` unit, before and after the panes) whose `ContextMenu` lists them. They fold in
`foldOrder`: the columns between the root row and the panes (oldest first), then the root row, then the
columns right of the panes (newest first), the table last (`foldSteps` counts the steps); shown columns never
fold. `useFolds` adds one fold per layout pass while the `nav` overflows (`scrollWidth > clientWidth`) and starts over when the width, the panes or a
crumb change. With nothing left to fold (`.tight`) names end in an ellipsis instead.

**Object header** (`ObjectHeader.tsx`): with a `place` (`ColumnPlace`: tab id and column index, passed by
`ObjectColumnView` and to the left side of `CompareView`) the table name is a button ("Open table <table>",
`object-header:table`) that shows the whole table in the same tab through `navigateTab`, and a column after the
first gets a close button ("Close column", `column-close:<index>`) that calls `popToColumn(tabId, index - 1)`.

**History** records the stack as the active column's address: `restacked(tab, stack, record)` (`slices/nav.ts`)
pushes the entry being left onto `nav` and keeps that address's stack in `stacks` (by `formatAddress`, pruned
to the object entries `nav` still holds). Back and forward restore the stack an address had (from `stacks`, or
from the entry's own `stack` when the tab was a table tab in between), else move the active column to it. Moving the active column records nothing.

**Keys** (`commands/columns.ts`, `COLUMN_COMMANDS`, rebindable): `object.down`/`object.downArrow` (`J`, ↓) and
`object.up`/`object.upArrow` (`K`, ↑) move the focus of a Document column through `focusOrder(document,
collapsed)` (`lib/document.ts`: the values in view order, collapsed groups and the repeating `events` left out);
`object.left`/`object.right` and their arrows move the active column (a hidden one shows); `object.drill` (Enter) opens the focused
value (`openValueAt`: one reference target or another row's key as the next column, the own key toggles the
view, serialized data as a value column); `columns.close` (Escape, scope `global`) restores a maximized stack, else closes the rightmost column;
`column.maximize` (⌘⇧↵, scope `object`, no conflict with `editor.runAll` in the `editor` scope) toggles
maximize of the active pane while `maximizable` holds. The focus, column and drill keys act only while the
keyboard focus is in `.stack`, the `.trail` or on nothing, never in inputs; the arrows and Enter leave a
focused grid or pane divider alone, Enter a focused button. `columns.close` and `column.maximize` work wherever
the focus is: `canClose` refuses while text is edited or a menu is open, `canMaximize` while a menu is open.

The pane keys are ⌘ combinations (scope `object`), which the registry fires in inputs too, so their `when`
refuses while text is edited (`isTextInput`: inputs, textareas, CodeMirror) or a menu is open; otherwise they act
wherever the focus is in the tab: `column.focusLeft` / `column.focusRight` (⌘⌥← / ⌘⌥→, while the stack has
several columns) make the neighbouring column active (`setActiveColumn`; from outside the stack they blur the
focused element first, so `useFocusActive` moves the focus into the new pane); `column.close` (⌘⇧W, while the
active column is not the first) runs `closeActiveColumn`; `column.openInTab` (⌘⌥↵) opens the active column's row
(`objectColumnAt`, so a value column's owner) with `openObject(address, { preview: false })`, offered while that
row is not the tab's first row (which `openObject` would reuse). None of them repeats while held. The peek's ↵
ignores ⌘ and ⌃, so ⌘⌥↵ with ⌥ pressed first does not also open the peeked target.

### Addresses and history

Every view has an address (`databee://<slug>/<database>/<table>/<key>?…`, [architecture](architecture.md#addresses)).
`lib/address.ts` ports Rust's parser and formatter (`parseAddress`, which throws an `AddressError`,
`formatAddress`, `sameAddress`), because addresses are built on every navigation; a Vitest
test checks it against `src/test/fixtures/addresses.json`, which a Rust test writes.

`lib/values.ts` turns a cell into a `Value` (kind and provenance): `valueAt(page, row, col)` for a browse
page (sourced from `page.table` and `page.keys[row]`), `resultValueAt(result, row, col)` for query
results. It reads the result's `kinds` and `serialized` cells and mirrors Rust's `value::ResultValues`
([architecture](architecture.md#values-and-the-lens)).

- **Tab addresses** (`slices/nav.ts`): a `TableTab`'s `address` is derived from its table and `where` by
  `tableAddress`: one match on the table's single-column primary key (from the `structures` cache) is the
  row (`…/<table>/<key>?view=rows`), anything else the table with its filter (`…/<table>?col=val`, a `null`
  value written as a bare `?col`). The typed SQL filter never enters an address. A `QueryTab`'s address is
  the database home (`homeAddress`), null without a current database. The connection part is the session's
  `slug`.
- **History:** every tab has `nav { back, forward }`, at most `NAV_LIMIT` (50) `NavEntry`s each: `{ kind,
  address, stack? }`, where `kind` (`table` or `object`) says which tab showed the address (a table tab
  filtered to its primary key has a row address too), and `stack` is the column stack of an object tab that
  became a table tab. `navEntry(tab, next)` builds the entry of the tab being left. `showRows` (opening a row in
  place, clearing the column filter), `navigateTab` and a replaced preview tab push the entry being left and
  clear `forward`; an unchanged entry pushes nothing. `navBack` / `navForward` (commands `nav.back` ⌘[ and
  `nav.forward` ⌘], global scope, and the toolbar's `NavButtons`) move a table or object tab to the entry,
  turning it into a tab of the entry's kind: a table entry restores its table and `where` (a row address
  needs the primary key, so they load the structure first), an object entry the stack its address had
  ([Column stack](#column-stack)). They are enabled only while `selectCanNav` finds history, so ⌘[ still
  outdents in the editor.
- **`navigateTab(tabId, address)`:** moves a table or object tab in place (same id, tab bar position, preview
  flag and history), recording the entry being left: a row address makes it an object tab with that row as its
  only column, a table address a table tab of it (Data view; a table tab keeps its view). The title is
  recomputed and the tab's grid selection cleared; the panel remounts with the kind, so no table page state
  survives. `wholeTable(address)` is a row's unfiltered table address. A value click handler moves the active
  tab with `const tab = selectActiveTab(s); if (tab?.kind === "table" || tab?.kind === "object")
  s.navigateTab(tab.id, address)`.
- **`openAddress(text, { preview? })`:** parses, finds the open session by slug, opens a row address as an
  object tab (`openObject`), a table address in a table tab (`openTableTab` with the matching `where`), in
  the preview tab when asked, the home address as the readout (`openReadout`), `?project` as the Project page
  (`openProject`), a saved query as a query tab (`openSavedQuery`) and `?find=` as the palette. The dev hook
  reaches it as
  `window.databee.getState().openAddress("databee://…")`.
- **Slug renames:** `renameConnectionSlug(identity, slug)` calls Rust, then rewrites `slug` on the sessions
  and the connection part of every address of their tabs. Settings → Connections lists the saved
  connections with their slugs (`components/settings/ConnectionSlugs.tsx`, anchors `settings:slug:<identity>`).

### Helpers and anchors

The Sidebar's "New query: SELECT …" asks Rust for the statement (`api.tableSelectSql`), so identifier
quoting stays in Rust too.

Interactive elements carry `data-stage`, `data-stage-label` and `data-stage-list` attributes. These are the
anchors for the [Stage hook](stage.md). Keep them stable when you change components.

`src/lib/` holds pure helpers, each with a test next to it:

| File | Contents |
|---|---|
| `format.ts` | bytes, counts (`formatCount`, `compactCount`), cell text (`cellDisplay` only for `Value`), TSV/JSON export, `relativeTime`, `isoUtc` (`Value`'s `TimeValue` overlay, `.time-peek`) |
| `values.ts` | `valueAt`, `resultValueAt`, `cellValue` (see [Addresses and history](#addresses-and-history)) |
| `coverageProblems.ts` | `coverageProblem(coverage)`: the problem text and row filter of a reference's coverage (values that point nowhere; 0 or NULL never count), `unresolvedCount` |
| `ruleForms.ts` | the Project page's form state (`ReferenceForm`, `KindForm`, `StyleForm`), prefills from a `RuleDraft` or a listed rule, `referenceRule` / `kindRule` / `styleRule`, `fieldError`, display texts (see [Project tabs](#project-tabs)) |
| `provenance.ts` | `provenanceText(value)` and `sourceText(provenance)`: the status bar text of a value |
| `cells.ts` | `CellSource` and `fullCellText`: a cell's full text, fetched for `longText`; `fetchStoredValue` (a value read again by its provenance row, [Column stack](#column-stack)); `columnIndex` (case-insensitive, as MySQL column names are); `singleKey` (a `RowKey` as the `keyColumn` and `key` of a row change; table tabs and Rows view edit by `BrowsePage.keys` from Rust); `DEFAULT_ROW_LIMIT` (the page size before settings load) |
| `useApiResult.ts` | `useApiResult`, the data loading hook (see [Store](#store), Data loading) |
| `clipboard.ts` | `copyText` with a success or error toast |
| `address.ts` | `parseAddress` / `formatAddress`, `sameAddress`, `address`, `SCHEME`, `AddressError`: the port of Rust's `address` module |
| `sql.ts` | statement ranges and the statement at the cursor |
| `theme.ts` | `applyAppearance` (scheme, the accent in effect, `--pad`, sizes, font families), `resolveScheme` / `useResolvedScheme`, `useSystemDark` (one shared `prefers-color-scheme` query), `useRowHeight` (`--row-h` measured once per appearance change), `syncRowHeight` (re-reads `--row-h` after `applyAppearance`), `applyInitialScheme` (first paint) |
| `codeTheme.ts` | CodeMirror theme and syntax colors from the scheme's `--code-*` tokens |
| `document.ts` | object addresses (`objectAddress`, `documentText`, `objectTitle`), `findValue` / `provenanceKey`, group texts (`relationText`, `groupOriginText`, `countText`, `moreText`), `groupAnchors`, `collapseKey`, `focusOrder` |
| `diff.ts` | change marks of live mode, snapshots and compare (`ChangeMarks`, `withDelta`, `documentMarks`, `compareMarks`, `withRemovedRows`, `FLASH_MS`; see [Live mode, snapshots, compare](#live-mode-snapshots-compare)) |
| `pins.ts` | a saved query's pin and the pin dialog's choices (`pinChoice`, `pinOf`, `pinLabel`) |
| `nativeMenu.ts` | `installNativeMenuGuard` (see [Boot](#boot)) |
| `columns.ts` | the column stack's types (`ObjectColumn`, `ValueColumn`, `StackColumn`, `ColumnStack`), `objectColumn`, `objectColumnAt`, `columnName` / `columnLabel`, `shownColumns`, `maximizable`, `paneShares`, `MIN_PANE_WIDTH`, `DIVIDER_WIDTH`, `DEFAULT_MAX_PANES` (see [Column stack](#column-stack)) |
| `tableCheck.ts` | the table tab's live check (`TABLE_CHECK_MS`, `RowChange`, `TableCheckState`, `PauseReason`, `TableCheckStatus`, `checkStatus`, `freshCheck`, `withHashes`, `rowsToRead`, `withRows`, `changeCounts`, `changesText`; see [Table tabs](#table-tabs)) |
| `trail.ts` | the trail's folding (`TrailUnit`, `TrailMore`, `trailUnits`, `foldSteps`; see [Column stack](#column-stack)) |

### Project tabs

A `ProjectTab` (`components/project/`) is the Project page of one database
([presets.md](presets.md#the-project-page)); one per session and database. `openProject({ sessionId?, database?,
form? })` (`project` slice) reuses it and replaces its `form` when one is given; `project.open` (global, no key),
the status bar's `presets …` button (`rulesInfo`, loaded per `dataVersion`) and `openAddress` of a `?project`
address call it. It has no history (`nav` stays empty).

- **Loading:** `loadProjectRules(sessionId, database)` keeps `api.projectRules` per `dataVersion` in
  `projectRules`; `changeProject(sessionId, database, change)` calls `api.projectUpdate`, then `bumpData`, so
  documents and grids reload with the new rules, and returns the `AppError` instead of showing it.
- **Layout:** `ProjectHeader` (preset switches, prefix, the accent field, export; its `children` are the
  section switch), then
  by `ProjectTab.section` (`setProjectSection`; default `overview`) either `Overview` or the per-table listing.
  `Overview` (`Overview.tsx`) renders `ProjectRules.changes` as `ChangeLine`s (the text opens
  `openTableTab(database, table, "structure")`; "edit" finds the listed row by `line.edit` and opens its form;
  the undo sends `line.undo` through `changeProject`), then the problems: `useAutoCoverage` over every
  applied reference, `coverageProblem` per counted one, and failing anomalies from the readout
  (`loadReadout`, the `readouts` cache); "Open rows" opens the source table with the coverage's `rows` filter.
  The per-table listing is a filter over `ProjectRules.groups` by table name (component state) and "Refresh
  coverage", then a `TableSection` per group: a band collapsed
  through `useCollapsedGroups(tab.address)` with the id `project:<table>` (the table's icon before its name),
  the table's `IconControl` ([Table icons](#table-icons)), `ReferenceLines` (rows and the
  `ReferencedByLine`), and from `RuleSections` `StyleLines`, `KindLines` and `EntityActionLines`. Rows get a
  `RuleSwitch` and `ActionButtons`; the Rust rows decide which (`id`, `origin`, `edited`). They share
  `ProjectContext` (`context.ts`) and the `:is(.object, .project)` band and row classes.
- **Coverage** (`coverage.tsx`): `useAutoCoverage(sessionId, database, references)` counts every applied
  reference the cache lacks with `api.referenceCoverage`, `COVERAGE_CONCURRENCY` (3) at a time; unmounting
  stops the queue and drops the counts still running. The cache is `coverage[session/database]` in the
  `project` slice (`setCoverage`, by `coverageKey` of the `ResolvedReference`), kept for the session;
  `refreshCoverage` empties it and bumps its `generation`, which reruns the hook.
- **Forms** (`lib/ruleForms.ts`): form state lives in the tab, so it survives switching tabs, and every opener
  (a row's "edit", `openProject({ form })` from a value) shows it as the same sheet: `FormFrame` renders a
  `Modal` panel with the palette's classes (`variant="panel"`, `palette rule-sheet`, Stage window `rule-form`); ⎋ closes through the
  modal, ⌘↵ saves from any field. `referenceFormFromRow` sets `replacing` for a row the project stores
  (`storedInProject`: project or `edited`) and `ruleId` to a preset row's id, so saving overrides it;
  `referenceRule` maps the real tables of the pickers to rule names through `ProjectRules.tables`;
  `fieldError` maps the path of `AppError.field` (`reference.when.meta_key`, `kind.in[0]`) to the form field
  it shows under. `ReferenceForm` previews coverage with `api.coveragePreview` 400 ms after the last change.
  `FormSheet` keys each form by type, index and opener, so editing another row remounts it.
- **Export:** `api.chooseSavePath(<slug>-<database>.toml, "toml")`, then `api.projectExportTo(…, path)`.

### Readout

The readout tab (`ReadoutTab`, one per session, first in the tab bar; `readout` slice) shows the home address of
a database (`databee://<slug>/<db>/`, [presets.md](presets.md#the-readout)). `connect` opens it for the
session's database, `openAddress` for a home address, the `readout.open` command for the active database;
`switchDatabase` moves an existing one to the new database. `Readout.tsx` (`components/readout/`) loads it
through `loadReadout(sessionId, database, force?)`: once per `dataVersion`, so counts refresh after every
apply, and again (`force`) whenever the tab is shown, since recent objects change without a data change. A
failed load shows `ErrorPanel`; a failed part or line shows its error in its place.

The tab uses its full width; tables align their columns with CSS subgrid (`.readout-table`):
- **Header** (`StatusHeader`, a `section` labelled "Status"): the database as the title, one meta line (host ·
  prefix · applied presets · server version), then each value-pinned query as a key/value row: the query's
  title (opens the saved query) | its `Value` | the value's source table.
- Then one `Section` per part: a band as in the object view (chevron, name, hint, count), collapsed and
  remembered through `useCollapsedGroups(tab.address)` under the keys `::anomalies`, `::created`, `::watched`,
  `::pinned`, `::recent`; a click on the band outside its buttons toggles it too.
- **needs attention** (region "Needs attention"), only when there are lines: one `.attention` card per
  anomaly: the count, the rule text with `preset · id`, a "Show rows" button (opens the saved query or the
  table with the anomaly's `where` as the typed filter) and the `fix` action's name as a button
  (`runTableAction`: the action sheet on the table address).
- **watched** (region "Watched values"), always (an empty line when nothing is watched; the band's hint says how
  to watch), between needs attention and recently created: one `WatchedRow` per
  `readout.watched` line: the label (`WatchedLabel`: opens the row when it was found; while renaming a
  `LabelField`, where ↵ or leaving saves once, a blank label resets it, ⎋ cancels) | the value (`CurrentValue`:
  `Value` in grid density, serialized data in full density so it expands in place; a `tree` as `ValueTree`, whose
  nodes watch `line.path` plus their keys; `missing` in italic `--o3` with its reason; an error in red) | the
  source (`sourceText` of `line.source` and the path, `--o3`) | rename and remove buttons, shown on row hover or
  focus (`labelWatched`, `unwatchValue`).
- **recently created** (region "Recently created"), when there are entities: `EntityChip`s (group
  "Entities"), each a toggle (`aria-pressed`, all on by default, kept in component state) with
  `countInWindow` or `failed`, and a chevron ("Query of <entity>") that discloses `EntityQuery` (table button,
  preset, SQL, error). Below, `readout.recentlyCreated.items` filtered to the shown entities, one `CreatedLine`
  each: relative time of `created` (when its kind is a timestamp with UTC; else the stored text; the tooltip
  names the stored value and its source column) | entity | table icon and `#key` (opens the row) | `SummaryLine`.
- **pinned**, always (an empty line when nothing is pinned; the band's hint says how to pin): count lines
  (count | title | SQL) and full-width row blocks (title, row count, SQL, rows); a title opens the saved query.
- **recently opened** (region "Recently opened"), only when there are objects: table | table icon and `#key`
  (opens the row) |
  `SummaryLine` | relative time.

Rows of a known table carry `data-row-table` ([Values](#values)), so hovering anywhere on them lights the
table in the sidebar.

Anchors: `readout`, `readout:value:<id>`, `readout:anomaly:<id>`, `readout:fix:<id>`, `readout:entity:<entity>`,
`readout:created:<row address>`, `readout:watched:<id>`, `readout:watched-rename:<id>`, `readout:watched-remove:<id>`, `readout:pinned:<id>`, `readout:recent:<address>`, `readout:collapse:<section>` (a band's chevron), and the tab
`tab:readout:<db>`.

### Connection health

Rust watches each session's server ([database.md](database.md#connection-health)); the page shows the
result and keeps what is loaded instead of failing on every click. What it looks like: [ux.md](ux.md#layout).

- **State.** The `health` slice (`src/state/slices/health.ts`) keeps `health`, the lost sessions by id.
  `setHealth(event)` applies a `session_health` event: a newly lost session toasts `Lost the connection to
  <name>`; one that is back is dropped, toasts `Reconnected to <name>` and, when it is the active session,
  reloads the view (`refreshView`, as ⌘R). Events change the state: they arrive in order, while a command's
  reply may overtake them; the one exception is an `ok` reply to a retry, which is applied as well, because Rust
  sends no event for a session it already considers healthy.
- **Retry.** `retryConnection(sessionId, quiet?)` calls `api.reconnectSession`; an `ok` reply goes through
  `setHealth`; when the reply is still `down` it toasts `Still no connection to <name>: <error>` (not when `quiet`). `retryLostSessions()` retries
  every `down` session quietly; `App.tsx` runs it on window `focus`. The command `connection.retry` (Retry
  connection, palette only, while the active session is `down`) runs `retryConnection`.
- **Toolbar.** `ConnectionHealth` (`src/components/ConnectionHealth.tsx`, after `ConnectionText` in
  `Toolbar`) renders only while the session is lost: the warning icon, the tooltip with the time, the error
  and the next automatic retry, and the Retry button (disabled, `Reconnecting…`, while one runs). Anchors:
  `toolbar:connection-health`, `toolbar:retry`.
- **Gates.** `blockedWhileLost(get, sessionId, what)` (exported from `state/store`) stops an action while the
  session is lost and toasts `Connection lost: can't <what> until it is back`; true when it stopped.
  `blockedUnlessLoaded(get, sessionId, address)` (`nav.ts`) lets a row through whose document is loaded
  (`documents`; Rows view always needs the server). Used by `runQuery`, `queueChange`,
  `queueActionPreview`, `applyChanges`, `undoLast`, `switchDatabase`, `refreshView`, `openTableTab` (a new
  tab; an open one only activates), `showRows` and `filterRows`, `setTableFilter`, `navigateTab`, back and
  forward, `openObject` (a new tab), `openEntity`, `drill`, and `TableTab`'s paging, sort and reload
  (`useBrowse`). Live polls skip a lost session (`live.ts`), `reloadValueColumn` keeps the value it read,
  and `loadReadout` keeps a loaded readout.
  Rust refuses whatever else asks the server with `connection_lost`, which `fail` turns into a short note.

### Rendering

Long lists are virtualized with `@tanstack/react-virtual`, so no view mounts more than a few dozen rows of a
list above 200:

- `ResultsGrid` virtualizes its rows inside its own scroll box: table and query tabs, and the Rows view, whose
  grids (`RowsView.tsx`) are 12 rows tall and browse up to the `rowLimit` setting.
- `LongList` (`components/LongList.tsx`) renders the rows of the page-scrolling views: Document groups
  (`groups.tsx`: fields, meta, rows, events), the Project page's references, value styles and kinds, and the
  readout's `RowsTable` (a pinned query shown as rows). Up to `LONG_LIST` (200) rows it renders them all;
  above that it renders the rows near the viewport of the nearest scrolling ancestor (found by its computed
  `overflow-y`), between two spacers, with the list's offset in that container as `scrollMargin`. Every row of
  a list must have the same height: the first rendered row is measured, `--row-h` until then. `revealWhere`
  scrolls the focused row into view as `j`/`k` move it. A windowed readout table sizes its columns in `ch`
  from every row (`charWidth`, `lib/format.ts`), not from the rendered ones. Rows that are not rendered have
  no `data-stage` anchors.
- Not virtualized, because they stay short: the palette (at most 80 items, `MAX_ITEMS`), the column stack
  (only the shown panes render), anomalies, the created timeline and recent objects.
  Rust caps most Document groups (`META_CAP`, 50 keys, and `ROWS_CAP`, 20 rows; `+n keys` opens the Rows
  view), but not the own row of a wide table or the `events` group, so `LongList` covers them all.

Store subscriptions return primitives or stable references (`useShallow` for filtered arrays), so hovering
re-renders only what shows the hover: the status bar's address slot (`hover.src`, else the focused value
from `selectFocusedValue`, whose text it builds in render). The linked table highlight is `TableFocus`'s
`<style>`, so neither the object view nor `GroupHeader` subscribes to `hover`.

Measured with React Profiler in jsdom (actual render durations, so the ratios matter, not the milliseconds);
`components/object/rendering.test.tsx` keeps the bounded window of meta keys and the zero commits on value
hovers as assertions:

| | order 4711 | 5,000-key meta group, before | after |
|---|---|---|---|
| key/value rows mounted | 5 | 5,003 | 54 |
| first render (sum of commits) | 24 ms | 352 ms | 35 ms |
| focus change (`focusObject`) | 0.8 ms | 54 ms | 1.6 ms |
| 50 value hovers: object view commits | 0 | 0 | 0 |

The `wp_write` fixture's post 9701 has 5,000 meta keys ([testing.md](testing.md#docker-fixtures)) for trying
the long group and the Rows grid in the app.

## Styling

The CSS is plain and global, imported in this order by `App.tsx`: `styles/fonts.css` (`@font-face` for the
bundled fonts), `styles/schemes/<id>.css` (one file per color scheme), `styles/theme.css` (derived tokens and
aliases) and `styles/app.css` (components, sectioned with `/* ---------- name ---------- */` comments). There
are no CSS modules. Icons come from `lucide-react`. Token names, values and roles are in
[ux.md](ux.md#color-schemes-and-tokens).

- **Color schemes.** A scheme file sets every scheme token under `[data-scheme="<id>"]`: surfaces, text,
  hairlines, shadows, the scrim, the accent hues `--a-*`, the value-style colors `--style-*` and the CodeMirror
  colors `--code-*`. `applyAppearance` sets `data-scheme` on `<html>` from `theme` (system, light or dark)
  and the `lightScheme` / `darkScheme` settings; `App.tsx` reapplies when macOS switches appearance. Rust
  (`window.rs`) sets the native window theme from `theme`. The
  selector is not tied to `<html>`, so Settings previews a scheme inside a swatch. Adding a scheme: a new file
  in `styles/schemes/`, its import in `App.tsx`, its id in Rust's `SCHEMES` and in `SCHEMES` in
  `src/api/types.ts`.
- **No color literals** (`#hex`, `rgb(`, `hsl(`) outside `styles/schemes/`. Everything else uses a token or
  a `color-mix()` of tokens.
- **Derived tokens.** `theme.css` derives the opacity levels (`--o2`, `--o3`, `--dimmed` at 0.3), the accent
  mixes (`--cell`, `--sel`, `--lit-solid`), `--accent` from `html[data-accent]` (default orange), the type sizes
  (`--fs-*`) and the row height `--row-h` from `--pad`. `--bg-quiet` is a scheme token (`schemes/*.css`), not
  derived.
- **Accent per database.** `data-accent` is the accent in effect, `selectEffectiveAccent` (`project` slice):
  the active session's current database's own accent, else `settings.accent` (also on the connect screen).
  `App.tsx` passes it to `applyAppearance(settings, systemDark, accent ?? settings.accent)`, so switching the session, the
  database or the accent applies at once. The Settings window always shows `settings.accent`: `Modal`'s `accent`
  sets `data-accent` on its sheet, and `theme.css` derives the accent tokens on any `[data-accent]` element. The project slice keeps `accents` by `accentKey(slug, database)`
  (`api.projectAccents`, loaded at startup and after `renameConnectionSlug`); every session of a connection
  shares them. `setAccent(sessionId, database, accent)` stores one through `api.projectSetAccent` and patches
  `accents`; nothing reloads. `selectDatabaseAccent(s, slug, database)` reads one. `components/Accent.tsx` has
  `AccentSwatches` (Settings → Appearance and the Project header; Stage anchors `<stage>:<accent>` in list
  `<stage>`) and `AccentDot` (toolbar, connection switcher), both colored by `--a-<accent>`. Old names that older rules in `app.css` still read remain as aliases
  onto scheme tokens; new CSS and components use only the new names.
- **Sizing.** Chrome is in rem: `html { font-size: var(--ui-font-size) }`, so ⌘= and ⌘- scale it. The row
  anatomy (the 20px line, `--pad`, `--row-gap`, `--row-inset`, `--focus-out`) and the window's own buttons
  (`--traffic-lights`, `--title-bar`) are in px.
  `--data-font-size` sizes the grid, the editor and the value trees. The row-padding setting (steps 1 to 4)
  sets `--pad` (1, 3, 5 or 8px); `--row-h` is a 20px line (more for large data sizes) plus twice `--pad`.
  `useRowHeight` hands the measured value to the virtualizer.
- **Fonts.** Geist (400, 500, 600) and JetBrains Mono (400, 500, 600, italic 400) are bundled as woff2 in
  `src/assets/fonts/` with their OFL license files. The `fontUi` / `fontMono` settings put an installed family
  in front of the bundled stack (`--font-ui-bundled`, `--font-mono-bundled`).
- **CodeMirror.** `QueryEditor` passes `theme="none"` and adds `codeTheme(dark)`, so the editor reads the
  `--code-*` tokens; `dark` (from `useResolvedScheme`) only picks CodeMirror's base styles.
- **macOS look.**
  - The toolbar leaves `padding-left: var(--traffic-lights)` (88px, not scaled) for the traffic lights.
  - `data-tauri-drag-region` marks the draggable areas.
  - `user-select: none` applies globally; inputs and `.selectable` opt back in.
  - Shortcut labels use the ⌘⌃⌥⇧ glyphs.

## Tests

- **Setup.** Vitest with jsdom (`vite.config.ts`). `src/test-setup.ts` loads the jest-dom matchers and stubs
  `ResizeObserver`, `matchMedia`, Range rects (for CodeMirror) and the clipboard. It also sets
  `navigator.platform` to Mac and fakes element sizes so virtualized lists render.
- **Mocking.** UI tests mock the `src/api` module:

  ```ts
  vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));
  const mocked = vi.mocked(api, true);
  mocked.execute.mockResolvedValue({ results: [], elapsedMs: 1, database: "db" });
  ```

  `createApiMock()` returns every `api` function as a `vi.fn()` with harmless defaults.
- **Fixtures.** `src/test/fixtures.ts` provides `connectedStore()`, `resultSet()`, `queryTab()` and cell helpers.
  `src/test/settings.ts` provides default settings, `session.ts` a `session`, `now.ts` the fixed time `NOW`,
  `iconFixtures.ts` table icons (`rulesInfoOf`, `showTableIcons`); `objectFixtures.ts`, `projectFixtures.ts`,
  `paletteFixtures.ts` and `columnFixtures.tsx` the data of their views.
- **Style.** Query by role or text, drive the UI with `userEvent`, and assert what is visible or which API
  call was made. Reset with `resetStore()` or `connectedStore()` in `beforeEach`.
