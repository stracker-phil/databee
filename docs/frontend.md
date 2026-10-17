# Frontend

`src/` holds a React 19 + TypeScript app, bundled by Vite. State lives in one zustand store. The frontend
renders and forwards intent; [Rust owns the logic](architecture.md#design-rules).

## Boot

1. `index.html` loads `main.tsx`.
2. `main.tsx` calls `installModules()` (`src/modules.ts`, see [Modules](architecture.md#modules)) before the
   first render:
   1. `installCore()`: `registerCoreSettingsPages()` and `registerCoreScopes()`.
   2. `installStage()` (`src/stage/install.ts`): the Presentations Settings page and the Stage event
      listeners, which stay subscribed for the app's lifetime.
   3. `installWp()` (`src/components/wp/install.ts`): the `wp` scope.
   4. `installCommands()`: the command registry. It runs last because some core commands use WordPress scopes.
3. `main.tsx` calls `installNativeMenuGuard()` (`src/lib/nativeMenu.ts`): a right-click that no app menu
   handled shows nothing, so the WebView's own Reload and Inspect Element never appear; text fields and the
   SQL editor keep the native Cut/Copy/Paste menu.
4. `main.tsx` assigns the store to `window.databee` only when `import.meta.env.VITE_DEV_HOOKS` is `"1"`, which
   `bun run build:app` and `bun run app` set for the debug bundle the dev scripts drive; `bun run build`
   (release) leaves it out. It renders `<ErrorBoundary><App/></ErrorBoundary>`.
5. `App.tsx` loads the two stylesheets and calls `useBackendEvents()` once:
   1. `loadSettings()` loads settings and keybindings. `installCommands()` runs again to apply the user's
      bindings, then `drainLaunches()` pulls the queued launch requests (`take_launch_events`).
   2. It subscribes to `launch` (drain again), `command` (menu clicks → `runCommand`) and `remote` (requests
      from `databee open` / `databee command` → `answerRemote` in `src/commands/remote.ts`, which always
      replies with `remoteReply`).
   3. It adds a capture-phase `keydown` → `handleKeydown`, plus `error` / `unhandledrejection` → error toast.
5. `applyAppearance(settings)` runs whenever settings change.

Layout:
- Without a session, the window shows `ConnectScreen`.
- With sessions, it shows `Toolbar`, `FavoritesBar` (Database mode), `WpWorkspace` or `DbWorkspace`,
  `PendingTray` and `StatusBar`.
- `Dialogs`, `CommandPalette` and `Toasts` always render on top.

```
App
├─ ConnectScreen                       (no session)
└─ .app grid
   ├─ Toolbar                          connection / database text (pickers), mode switch, Path, search field (palette), icon buttons
   ├─ FavoritesBar                     (Database mode)
   ├─ WpWorkspace                      (WordPress mode, see wordpress.md)
   │  or DbWorkspace: [Sidebar |] TabBar + TabPanels
   │       TabPanels → QueryTab (QueryEditor, ResultsPane: ResultsGrid | ErrorPanel) [| value column]
   │                 → TableTab (filter bar + ResultsGrid | StructureView) [| value column]
   │                 → ObjectPanel: Strip | column (ObjectColumnView: ObjectHeader, DocumentView | RowsView,
   │                                or ValueColumnView) per stack column
   │                 → ProjectPanel (presets and prefix, rule form, references, value styles, kinds)
   ├─ PendingTray
   ├─ StatusBar                        ▤ sidebar toggle, provenance or address, presets, pending n
   └─ PeekOverlay                      while ⌥ is held
Dialogs (ShortcutsDialog among them) · CommandPalette · Toasts
```

## Store

One zustand store with no middleware, composed from slices. `src/state/store.ts` is only the composition
root: it creates the store, keeps the slice registry and re-exports the selectors, `fail`, `flushUiState`
and the state types, so components import everything from `state/store`.

- `src/state/slices/*.ts`: one file per slice (`settings`, `session`, `tabs`, `changes`, `data`,
  `favorites`, `ui`, `nav`, `objects`, `columns`, `project`, and the temporary `wp` until it moves to the WordPress module). Each exports its
  member interface and a `Slice<T>`: `create(set, get)` returns the initial state and actions,
  the optional `sessionClosed(sessionId, state)` returns what to drop when a session closes.
- `Store` is an interface extending every core slice's interface. A module adds its members with
  `declare module "../state/store" { interface Store { … } }` and registers with `registerSlice(slice)`,
  which merges `slice.create(set, get)` into the live store, so registering after creation works.
- `resetStore(patch)` (tests) re-runs every registered slice's `create`, then applies `patch`.
- `src/state/events.ts`: `onSessionEvent(handler)` (returns the unsubscribe function) and
  `emitSessionEvent`. `databaseChanged { sessionId, database }` fires after `switchDatabase` and when
  `loadDatabases` reloads the list of a session with a current database; the `wp` slice subscribes and
  runs `loadWp`. `sessionClosed { sessionId }` fires after `disconnect`'s update.
- `src/state/selectors.ts`: the selectors; `src/state/requests.ts`: `fail` and the request tokens.
- Slices import each other's reset helpers (`resetSelection`, `resetMode`, `resetWpNav`,
  `resetWpForDatabase`) and pure helpers (`activate`, `remembered`, `nextActiveSessionId`), never the store
  object, except where a module-level helper needs it (`fail`, the `wp` event subscription).

| Key | Meaning |
|---|---|
| `settings` | `Settings \| null`; null until loaded |
| `sessions`, `activeSessionId` | open connections (`SessionInfo`, with the connection `slug`) |
| `tabs`, `activeTabId` | `QueryTab` (sql, running, response, error, `valueColumn?`), `TableTab` (database, table, data/structure view, filter, structured `where`, preview, `valueColumn?`) `ObjectTab` (the column stack `columns`, the `active` index, the recorded `stacks`, preview; [Column stack](#column-stack)) or `ProjectTab` (database, the open rule `form`; [Project tabs](#project-tabs)). Every tab carries `address` and `nav` ([Addresses and history](#addresses-and-history)) |
| `documents` | `document` replies by `session/<address or shorthand>`, tagged with their `dataVersion` (`loadDocument`, `keepDocument`, `selectDocument`) |
| `schema` | per session: databases and tables per database |
| `structures` | `describeTable` replies by `session/database/table`, tagged with the `dataVersion` they belong to (`loadStructure`, `selectStructure`) |
| `mode`, `modeChosen` | `"wp" \| "db"`. A WordPress database switches to `wp` until the user picks a mode |
| `wp`, `wpNav` | detection results per `session/db`; navigation stack (max 100) of the WordPress screens |
| `dialog` | one dialog at a time, as a discriminated union (settings, connect, password, confirm, prompt, insertRow) |
| `pending`, `applied`, `applying` | plans Rust keeps (`PlannedChange`: id, description, statements for display, `canUndo`); the last 20 applied batch ids, for undo; an apply or undo is in flight |
| `selectedCell` | `{ tabId, row, col, column, cell, source, value? }`, the selected grid cell of a tab (in an object tab: of its active column). `source` (`{ sessionId, resultId }`, `lib/cells.ts`) lets a value column and the value commands fetch a `longText` cell in full; `value` (set by grids) is the focused value of the status bar and the value commands |
| `hover` | `{ src, table, linked }` under the pointer (`setHover`, from `useValueHover`): a value's provenance, or a table link (`linked`: a sidebar row, a section band); the status bar shows `src`, the linked table highlight reads `table` |
| `queryCounter` | query tabs opened per session, for unique default titles (`Query 3`) |
| `dataVersion` | bumped by ⌘R (`refreshView`, which first drops the session's schema cache with `refreshSchema`) and after apply or undo; views re-fetch when it changes |
| `historyVersion` | bumped when `runQuery` finishes; the history panel reloads on change |
| `trayFocusRequest` | bumped by `focusPendingTray()`; `PendingTray` focuses its Apply button (the status bar's `pending n` button uses it) |
| `uiState` | per connection identity: column widths, pane sizes (the WordPress detail pane, the SQL editor), `sidebarHidden` and `collapsedGroups` (`ui_state.json`); `toggleSidebar()` flips the active connection's flag, `toggleGroupCollapsed` an object group (`selectCollapsedGroups` reads them) |
| `paletteOpen`, `keybindings`, `favorites`, `toasts`, `recentNav` | as named |

Conventions:
- Components subscribe with narrow selectors (`useStore((s) => s.x)`), and `useShallow` for derived arrays.
- Event handlers call actions through `useStore.getState()`.
- Selectors live in `selectors.ts`: `selectActiveSession`, `selectActiveTab`, `selectCanNav`, `selectStructure`,
  `selectActivePending`, `selectCanUndo`, `selectSidebarHidden`, `selectVisibleDatabases` (the active session's databases without
  system schemas unless shown or current; use with `useShallow`), `selectWp`, `selectWpRoute`. For
  `selectWp`, `undefined` means not probed yet and `null` means not WordPress.
- Store actions wrap API errors in `fail(e, context)`, which shows an error toast and writes the message to
  the Rust log. Components that call `api` directly use the exported `fail` too, and update the UI only
  after the call succeeded (removing a recent connection, clearing history).
- Clipboard: `copyText(text, what)` (`lib/clipboard.ts`) is the only clipboard writer. It toasts
  "Copied {what}" or that the clipboard is not available.
- Formatting (`lib/format.ts`): `formatCount` for every count shown in full (`12,345`), `compactCount`
  for tight spots (`1.2k`, `3.4M`), `relativeTime(isoOrUnixSeconds)` for past and future times
  ("2 h ago", "in 3 d").
- Overlays: `Modal` traps Tab inside the sheet, focuses the first control unless a child has `autoFocus`,
  and returns focus to the opener on close unless the closing action focused something else. It handles
  Escape in the capture phase and stops it there, so only the sheet closes (keys inside `[data-recording]`
  pass through). The palette renders inside `Modal` too. `ContextMenu` focuses its first item, moves
  with ArrowUp/Down, closes on Escape or an outside click, and positions itself from its measured size.
- Writes: `queueChange` asks Rust to plan (`planChange` for row edits, `wpPlanChange` for the WordPress screens'
  edits) and keeps the returned plan; `queueAction(address, action, params)` plans a rule action (`planAction`); `applyChanges` sends the active session's plan ids, `undoLast` the newest batch id
  (`undoBatch`); `discardChange` / `discardAll` drop plans in Rust too. The page never sends SQL to write.
- In-flight guard: `applyChanges` and `undoLast` return early while `applying` is set; they set it before
  the first `await` and clear it in `finally`. `PendingTray` disables Apply and Undo meanwhile. `runQuery`
  refuses a tab that is already `running`.
- Request tokens (`beginRequest(key)` / `isLatest(key, token)`): only the newest request per key writes its
  reply. `loadFavorites` uses one key for all sessions; `loadWp` uses `wp:<session>/<database>` and switches
  to WordPress mode only while that session and database are still active.
- Selection: `selectCell` tags the cell with the active tab. `clearSelectionFor(state, tabId)` drops it when
  its tab closes or is left (`activate`); a `dataVersion` bump (`bumpData`) clears it outright.
- `disconnect(sessionId)` closes the session in Rust, then applies one `set` that merges every registered
  slice's `sessionClosed` result: the session and the next active session (`session`), its schema and
  `structures` keys (`session`), its tabs, `queryCounter` and the selection if its tab was closed (`tabs`),
  its `pending` and `applied` entries (`changes`), its `documents` (`objects`), its `wp` keys, and `wpNav` when it
  was active (`wp`). A slice that keys data by session implements `sessionClosed`.
- `switchDatabase` resets `wpNav` and the selection; unless the target is known WordPress it
  also resets `mode` to `db` and `modeChosen`. `connect` resets `wpNav`, `mode` and the selection and reuses
  the session's query tab on reconnect.
- Tab updates are typed: `updateQueryTab` and `updateTableTab` only patch tabs of their kind.
- UI state (`setTableWidths`, `setPaneSize`, `toggleSidebar`) is written debounced; `flushUiState` writes it on
  `beforeunload` (registered in `App.tsx`).
- Some errors become state instead: a password dialog, a tab's `ErrorPanel`, a confirm dialog for
  `confirmation_required`.

## API boundary

Only `api.ts` files import `@tauri-apps/api` or `@tauri-apps/plugin-*`: the core's is `src/api/index.ts`, a
module's is `src/<module>/api.ts` (Stage: `src/stage/api.ts`) ([ADR 0002](adr/0002-ipc-in-api-files.md)). `src/api/index.ts` exports `api`, a flat object of thin `invoke` wrappers with no logic, plus the event
subscriptions `onLaunch`, `onCommand` and `onRemote`. The IPC types are generated from Rust with `ts-rs`
([ADR 0014](adr/0014-generated-ipc-types.md)) into `src/api/types.gen.ts` (a module's types into its own folder, e.g. `src/stage/types.gen.ts`); never edit them. `bun run types`
regenerates them (it runs the `export_bindings_*` tests), and `bun run lint` fails when a committed file is
stale. `src/api/types.ts` re-exports the generated types plus the few frontend-only ones (`Keybindings`, `PaneSize`, `EMPTY_UI_STATE`, `TEXT_SIZES`, `SETTINGS_LIMITS`, `LOG_LEVELS`,
`ACCENTS`, `SCHEMES`, `BUNDLED_FONTS`). `ACCENTS` and `SCHEMES` mirror Rust's lists of the same name. `TEXT_SIZES` and `SETTINGS_LIMITS` mirror the clamps in Rust's `Settings::sanitized`, which stays the
source of truth; change both together. Command signatures in
`api` stay hand-written. `toAppError(e)` normalizes rejections to `AppError` and accepts only a known `kind`. Derived connection facts come from Rust and are never recomputed in the page: the
Keychain `account` (which also scopes favorites and history) is on `SessionInfo`, `RecentConnection` and login
errors (`AppError.account`).

## Commands, shortcuts, palette, menu

Every user action that has a shortcut or appears in the palette is a `Command` (`src/commands/registry.ts`):

```ts
{ id, title, scope, defaultBinding?, when?, run, menu? }
```

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
  called by `installCommands`); the WordPress lens registers `global` → `wp` from
  `components/wp/install.ts` (`installWp()`, called by `installModules()` before `installCommands`). Registering a
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
  Recent, row hits (an `Address` item for `databee://…`, then Rust's sections), Tables, Commands (those
  whose `when()` passes, plus the active object's rule actions, `objectActionCommands()`).
  - `parsePrefix` (`palette/parse.ts`) reads `>` (commands), `t:` (tables), `q:` (saved queries; none until
    U11), `#`, `@`, `=` and `databee://` (the whole text is the needle). `searchesRows` says when Rust is asked:
    `#<digits>`, `@`/`=` with 2+ characters, free text with 6+ (`MIN_TEXT`); shorter text matches only tables
    and commands (substring `score`).
  - `useRowHits` calls `api.paletteSearch` 200 ms after the last keystroke; a newer input drops older
    replies. The hits come in Rust's sections, titled by what they are (`objects with id 4711`, `rows that
    reference 4711`, `people matching jane`, `external id …`, `<table>.<column>` for `=`; see
    [architecture](architecture.md#palette-search)).
  - `PaletteList` renders a hit as its `source` (table, or `table.column`) in `--o2`, `#` and its key in the
    accent, then its detail `Value`s; other items as title, badge and key cap. It scrolls the active item into
    view. `PaletteFooter` lists the keys and the prefixes, and for `#` without entities says that it reads
    only tables under 1M estimated rows.
  - Enter on a hit (`openHit`) drills into the active object tab's stack (`drill` after its active column),
    else opens an object tab; ⌘↵ (`goAlt`) always opens a tab. A hit whose address is not a row opens with
    `openAddress`. Tables open in the preview tab, pinned with ⌘↵. Holding ⌥ peeks the selected hit
    (`PeekCard` under its row). `openAddress` of a `?find=<value>` address opens the palette with `=<value>`,
    and so does "Find everywhere" on a WordPress field.
- **Menu.** Native menu items emit `command` events. See [backend.md](backend.md#native-menu) for the Rust side
  of `menu: true`.

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
| Shell | `Toolbar`, `Path` (the column stack's crumbs), `FavoritesBar`, `StatusBar`, `PendingTray`, `Splitter` (resizable panes, persisted per connection), `ErrorBoundary`, `Field` (`InsetField`: the macOS field frame with icon and key cap; `KeyCap`) |
| Connect | `ConnectScreen` / `ConnectForm`: TCP or socket, recent connections |
| Database workspace | `Sidebar` (tables tree with filter and arrow-key navigation over a roving `tabIndex`, history reloaded on `historyVersion`; table rows of the current database carry `data-table` and take `lit` / `lit-value`), `Tabs` (a table or query tab inside `WithValueColumn`, which adds its value column on the right without moving the tab in the tree), `QueryTab`, `QueryEditor` (CodeMirror, MySQL dialect, schema completion), `ResultsGrid` (virtualized, resizable, sortable, keyboard navigation, copy as TSV/JSON, inline edit; its hooks and `fitWidth` in `gridHooks.ts`), `TableTab` (paged browse, WHERE/id filter, column filter chip, structure view), `ErrorPanel` (query and load errors), `ValueTree` (PHP/JSON decode) |
| Values | `value/Value.tsx` (`Value`: one stored value drawn by its kind), `value/useValueHover.ts` (the status bar's hover listener), `value/useOpenValue.tsx` and `value/open.ts` (what a click or Enter on a value opens, `ColumnContext`), `value/ValueColumnView.tsx` (a value column), `value/Peek.tsx` (`PeekOverlay`), see [Values](#values) |
| Objects | `object/`: `ObjectPanel` (the stack, strips), `ObjectColumnView`, `ObjectHeader`, `DocumentView`, `RowsView`, `GroupHeader`, `AlsoLinked`, `EntityDocument`, `useViewScroll`, `useCollapsedGroups`, see [Object tabs](#object-tabs) and [Column stack](#column-stack) |
| Project | `project/`: `ProjectPanel`, `PresetsSection`, `ReferenceForm`, `KindStyleForms`, `ReferencesSection`, `RuleSections`, `coverage`, `formParts`, see [Project tabs](#project-tabs) |
| Overlays | `Modal`, `Dialogs` (the insert dialog sets each column to a value, NULL or its default; default leaves it out of the INSERT), `SettingsDialog` (see [Settings pages](#settings-pages)), `ShortcutsDialog` (`?`: the bound commands of the scopes active when it opened, most specific first, from the registry), `CommandPalette` (with `palette/`: `parse`, `usePaletteItems`, `useRowHits`, `PaletteList`), `ContextMenu`, `Toasts` |
| WordPress | `components/wp/`, see [wordpress.md](wordpress.md#screens-until-u12) |

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

`App.tsx`: `Workspace` returns the connect screen, the WordPress shell or the database shell (`AppShell`
wraps the toolbar, pending tray, status bar and `PeekOverlay`). In `DbWorkspace`, `main` (`TabBar` +
`TabPanels`) keeps the same tree position whatever the sidebar does: a hidden sidebar (`view.sidebar`,
`selectSidebarHidden`) leaves an empty slot and adds `no-sidebar`, so CodeMirror history and table pages
survive. The sidebar width is the `--sidebar-width` token in `theme.css`. There are no inset panels: chrome
(`--chrome`) and content (`--bg`) meet at `--line` hairlines. Values open as columns, never in a docked pane.

- **Toolbar:** `ConnectionText` shows `connection / database` as mono text; a transparent native select over
  each name (`Connection` only with several sessions, `Database` always) opens the picker. `Path` shows the
  active object tab's stack (see [Column stack](#column-stack)). `SearchField` is a
  button styled as a field (label "Go to") that opens the palette; its key cap follows `palette.open`.
- **Status bar:** `SidebarToggle` (▤, `aria-pressed` while the sidebar shows), `Address` (the provenance of
  the hovered or selected value while there is one, see [Values](#values); else in WordPress mode the
  table prefix; otherwise the active tab's address as text, a button that copies it, with `· filtered` when a
  typed SQL filter applies; a query tab adds rows, time and statement count, with the row-limit warning), spacer, `PendingCount` (`pending n`, a button to the tray when n > 0).
  The theme chip is gone: the theme stays in Settings and the palette.
- **Sidebar:** the Tables/History switch, the filter (`InsetField`, key cap from `sidebar.focusFilter`), a
  `db · N tables` caption, then the tree. Rows are `.si` items (`sel` for the active table). All tabs of the active session
stay mounted; inactive panels are `hidden`. Tabs use a roving `tabIndex` with Left/Right arrows
(`TabItem`, `tabIcon`). Shortcut hints (new tab, run statement, run all, editor placeholder) come from
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
(`drill`). A secret reveals on its own. A style
sets `color: var(--style-<name>)`. CSS classes are `val` and `val-<kind>` (`styles/app.css`, values).

Every value carries `data-src` (`provenanceText`) and `data-table` (its provenance table). `useValueHover`
(`AppShell` in `App.tsx`) adds one delegated `mouseover`/`mouseleave` listener to the app, reads the nearest
`[data-src]` or `[data-table]` at most once per animation frame and writes `hover` (`setHover` skips equal
values): a `.val` element is a value (`linked: false`), anything else a table link (`linked: true`; a sidebar
row, a section band, whose `data-src` is the rule's origin). The status
bar's address slot shows `hover.src`, else the provenance of `selectedCell.value`, else the address.
Interactive values with a provenance are Stage anchors `value:<table>.<column>:<key>` (list `values`).
A reference with one target also carries `data-ref` (the target address), `data-via` (its column) and `data-by`
(the rule, `bySourceText`) for the peek.

**Peek** (`value/Peek.tsx`, `PeekOverlay` in `AppShell`): window listeners in the capture phase. A keydown of ⌥
alone (no dialog or palette open) peeks the hovered `[data-ref]` element, else the focused value
(`selectFocusedValue`) when it is a reference with one target; moving the pointer onto another reference while
⌥ is held peeks that one; keyup or a window blur hides it. The card sits below the element (fixed, kept in the
window), reads `api.peek(sessionId, address)` (`Peek { keyColumn, values }`), cached by session, `dataVersion`
and address, and draws the values with `Value` (grid density). ↵ while it shows opens the target with
`openTarget` (the next column when the reference is in a stack column).

**Value commands** (`commands/values.ts`: `VALUE_COMMANDS`, ids `value.*`, scope `table`, and
`OBJECT_VALUE_COMMANDS`, ids `object.*`, scope `object`) act on the focused value, `selectFocusedValue`: the
selected grid cell of the active tab, else the active column of an object tab: the focused value of its Document view, or a value column's value. They are rebindable
like every command.
Bare keys never fire in inputs or the editor (`resolveKeydown`).

| Command | Key | Does |
|---|---|---|
| `value.copyRaw` | `y` | copies the stored text (a `longText` cell fetched in full; NULL as empty) |
| `value.copyCondition` | `Y` | `api.valueCondition(sessionId, database, provenance, raw)`: Rust renders the condition from `ColumnEquals` with the session's `Dialect`; on the value column of a `[[meta]]` table it reads the row's key and returns `` `meta_key` = '…' AND `meta_value` = '…' `` |
| `value.edit` | `e` | dispatches `EDIT_VALUE_EVENT`; the grid of the active tab that holds the selection, or the active Document view, opens its inline editor there, which queues a core `setCell` on the provenance row through the pending tray (documents: rows with a single-column key, meta rows by their own id) |
| `value.filter` | `f` | in a table tab, `filterRows(tabId, { column, value })` (nav slice): adds the match to the table tab's structured filter, replacing one on the same column, through `showRows` (so ⌘[ returns); elsewhere it opens the value's table filtered by it in the preview tab |

| `value.addReference` | `r` | `openRuleForm("reference")`: `api.ruleDraft(sessionId, database, provenance)` (the column as rules name it, the meta key on a meta value), then `openProject` with a prefilled `ReferenceForm` |
| `value.setKind`, `value.setStyle` | | the same with a `KindForm` or a `StyleForm` (the style takes the value's text) |

Binary cells offer none of `Y`, `e`, `f` and the rule commands, since their cell holds only a preview. The grid's
cell menu and the Document view's value menu (right-click) list `RULE_MENU`: "Add reference…", "Kind…",
"Style…".

**WordPress options screen (until U12).** `FieldValue` renders an option field through `Value` when its kind has
a form there (NULL, `""`, secrets, money and time hints, short text); entity links (a reference-style button that
runs `openEntity`), editable trees and collapsed long text keep the old rendering.

### Results grid

`ResultsGrid` resets its selection and inline editor when `rows` changes identity and clamps indices
before use. It has no row-number column. Header cells show the column name with the sort arrow, and the type
below it. Hover is plain CSS (`--row-hover` on the row, `--cell` on the cell), so crossing cells never
re-renders the grid. Cells render `Value` with `density="grid"`, from the `valueAt(row, col)` prop
(`TableTab`: `lib/values.ts` `valueAt` on the page; `QueryTab`: `resultValueAt`; without it `cellValue`);
values are built once per row and rows identity, so `Value`'s memo holds while the grid re-renders. The
only kind class on a cell is `num` (right alignment for number cells). Clicks go through `useOpenValue(owner)`:
a primary key opens its row as an object in the preview tab, a reference opens its target with
`openAddress(…, { preview: true })` (several targets: a menu), an enum runs `value.filter`, serialized data
opens a value column (in an object tab's column: the next column of the stack, through `ColumnContext`). On a
primary key cell, double-click opens the object pinned and Enter in the preview tab (in a stack: the next
column); Enter on a reference with one target opens it, on serialized data or long text a value column with the
full text; otherwise double-click and Enter edit. Keys live in `KEY_ACTIONS` (arrows, Home/End,
PageUp/PageDown, Enter, ⌘C). `owner` names the object whose rows a grid shows (Rows view), `focus` selects and
reveals a cell (the focus carried over from the Document view). `bytes`
cells are never editable: they show a `0x…` preview, which an edit would write back.

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
`session/database/table`, one `describeTable` per `dataVersion`, purged on disconnect); `DataView` and
`StructureView` share it. `DataView` browses only once the structure (or its error) is known, so an ID
filter always has the primary key. `useBrowse` drops replies that are not the latest request, and the
`Pager` moves by `BrowsePage.limit` (Rust's clamped page size). A failed structure shows an alert and
leaves editing off. `FilterBar` and `useRowEditing` hold the filter input and the row menu and cell edits;
primary-key and WordPress id columns match case-insensitively. The typed filter and `where` live on the
tab, not in `useBrowse` (which keeps only offset and sort), so back and forward can change them; other
rows start at the first page. Applying the filter calls `setTableFilter` (no history entry: a free SQL
filter is not addressable). The tab's `where` goes into every browse request next to the typed filter;
`ColumnsFilterChip` shows `BrowsePage.columnsFilter.label` read-only (the SQL as its tooltip), and its clear
button calls `showRows` with no matches, which records the filtered address for back.

### Opening rows

The page never builds SQL conditions. Opening rows in the Database view passes structured column matches
(`ColumnEquals`: `{ column, value }`, a `null` value means `IS NULL`), and Rust renders and quotes them
([Browsing tables](database.md#browsing-tables)):

- `openRow(table, column, value)` (tabs slice): while the Database view shows a table tab of the active
  session, that tab moves to the rows (`showRows`, so ⌘[ returns). Otherwise it switches to Database mode
  (`setMode("db")`) and opens `table` of the active database with `where: [{ column, value }]` in the
  preview tab. Option rows use it.
- `openInDatabase(table, where?)`: the same with any number of matches, in a kept tab. `RowSource.where`
  (`FieldValue.tsx`) carries the matches for a field's "Open row in Database view".
- `openTableTab(database, table, view?, filter?, { preview?, where? })`: an open tab is reused only when
  its `filter` and `where` are the same. A preview tab that replaces the previous one takes over its history
  and records the replaced address, so a single click in the sidebar can be undone with ⌘[.

### Object tabs

An `ObjectTab` shows rows as a stack of columns (`components/object/`, mock `Main`, `Rows` and `Drill`; see
[Column stack](#column-stack)). Each object column (`ObjectColumn`, `lib/columns.ts`) has its row `address`,
whose `view` mirrors the column's `view` (`null` for the document, `rows`), and `focus`, the provenance of its
focused value. The tab's `address` is the active column's. `openObject(address, { preview })` (`objects` slice)
reuses the tab whose first column is that row (and makes that column active) and otherwise opens one, in the
preview tab when asked: a preview of either kind (table or object) replaces the session's
previous preview and takes over its history (`previewTab`, `placed`). Rows open as objects from a primary key
click, from a grid's double-click or Enter on a primary key cell, from `openAddress` with a row address, from
the table tab's row menu ("Open as object") and from `openEntity(entity, id)` (WordPress records, see
[wordpress.md](wordpress.md)).

- **Loading:** each column (`ObjectColumnView`) calls `useDocument(sessionId, text, database)`, which reads
  `documents` and calls `loadDocument` once per `dataVersion`, so ⌘R and applied writes refresh it; the previous
  document stays while reloading. The key is the address without its view (`documentText`), so toggling never
  refetches. A failed load renders `ErrorPanel`. The tab title becomes `<entity> #<key>` of the first column once
  its document names its entity.
- **`ObjectHeader`:** table name, `#key`, the Document/Rows segmented control with the `object.toggleView` key
  cap, the actions button, the summary (`Document.header`) as `key value` pairs, other rows an entity
  shorthand matched (`alternatives`). Right-click opens the actions menu (`actionMenu`): each action plans
  through `queueAction` after a prompt per required parameter (choices in the title; optional parameters keep
  their default). The palette lists the same actions.
- **`DocumentView`:** per group a `GroupHeader` band (`data-table`, relation text from `relationText`, count,
  the rule's origin as `data-src`; the table name opens the group's rows in a table tab), then the body:
  `FieldsGroup` and `MetaGroup` as `.row.kv` rows, `RowsGroup` as a `.row.hdr` header plus rows on a grid
  template sized in characters, `EventsGroup` as time, table and column or key. Values render through the
  memoized `Field` (no effects per value); a mousedown focuses its provenance (`focusObject`), the focused
  value gets an accent outline on `--sel`, and stays in view as `j`/`k` move it. `+n keys` / `+n rows` under a
  capped group switches to Rows and scrolls to that group. Zero-row groups collapse into `AlsoLinked` ("Also linked: …").
- **`RowsView`:** per group with a table its band, the `WHERE` of its rows address, and a compact `ResultsGrid`
  (12 rows high, then it scrolls): a browse of the group's `rowsAddress` with the table's full columns and
  editable cells, or, for a group no address names (patterns), its document rows read-only.
- **Toggle:** `object.toggleView` (`Tab`, scope `object`; never in inputs, the editor or dialogs) and the
  segmented control call `toggleObjectView(tabId, index?)` / `setObjectView(tabId, view, focus?, index?)` on the
  active column (or the column of the control), which push the address being left onto the tab's history.
  Focus follows the provenance: Document → Rows selects the same cell in its group's grid, Rows → Document
  takes the selected cell's provenance as `focus`. Both views of a column share its `.object` scroll container
  (`overflow-anchor: none`), so `useViewScroll` keeps each view's own offset per column: a view starts at the
  top the first time, gets its offset back later (re-applied while its grids still load), then reveals the
  focused value. A column that collapses to a strip and back starts at the top again.
- **Collapse:** every `GroupHeader` has a chevron button (`aria-expanded`, "Collapse/Expand <table>"); it and a
  click on the band outside its buttons call `toggleGroupCollapsed(identity, database, "<document table>::<group id>")`
  (`ui` slice), stored in `UiState.collapsedGroups` by database (`ui_state.json`, debounced), so the choice holds
  for every row of the document's table. `useCollapsedGroups(owner)` reads it for both views. A collapsed group
  keeps its band and drops its body and `+n` button; in Rows its grid does not mount, so nothing is browsed. A
  click or key in the view ends `useViewScroll`'s offset restore, so collapsing does not jump back.
- **Linked table highlight:** a section band takes `lit` while `hover` is a table link to its table; sidebar
  rows take `lit` for a link and `lit-value` (lighter) for a hovered value of that table.

### Column stack

`ObjectTab.columns` (`StackColumn[]`, `lib/columns.ts`) holds `ObjectColumn`s and `ValueColumn`s
(`{ value, text, database }`: a stored value and its full text, after a queued edit the edited text); `active`
is the column `h`/`l` move, the path marks and the value commands act on. The `columns` slice
(`slices/columns.ts`) changes it:

- `drill(tabId, from, target)`: `target` as an object column after column `from`, closing any to its right,
  active. Clicks and Enter reach it through `ColumnContext` (`value/open.ts`), which each column provides.
- `openValueColumn(tabId, from, column)`: the same for a value column; for a table or query tab (`from` null)
  it sets the tab's `valueColumn`, which `WithValueColumn` (`Tabs.tsx`) renders right of the tab.
  `updateValueColumn` keeps an edited text.
- `popToColumn(tabId, index)` (a strip, a crumb), `closeRightmostColumn()` (`Esc`, `columns.close`; a table or
  query tab's value column first), `setActiveColumn(tabId, index)` (a mousedown in a column, in the capture
  phase so the column is active before a grid selects), `moveActiveColumn(delta)` (`h`/`l`). A new active
  column drops the tab's grid selection.

`ObjectPanel` is the stack: it measures its width (`ResizeObserver`) and `shownColumns(count, active, width)`
picks the columns shown in full (at least `MIN_COLUMN_WIDTH`, 420px, next to 34px strips for the others,
ending at the newest unless the active column lies further left). The rest render as `Strip` buttons with a
vertical `table #key`; one left of the active column pops back to it, one right of it becomes active. The active
column (`aria-current`) gets a 2px accent line on top when there are several, and takes the keyboard focus
(`tabIndex=-1`) when nothing outside the stack holds it.

**Path** (`Path.tsx`, in the toolbar): one crumb per column, the name from the column's document entity, else
the table's only entity in `rulesInfo.tableEntities`, else the table name; every crumb but the last pops to its
column.

**History** records the stack as the active column's address: `restacked(tab, stack, record)` (`slices/nav.ts`)
pushes the address being left onto `nav` and keeps that address's stack in `stacks` (by `formatAddress`, pruned
to the addresses `nav` still holds). Back and forward restore the stack an address had, else move the active
column to it. Moving the active column records nothing.

**Keys** (`commands/columns.ts`, `COLUMN_COMMANDS`, rebindable): `object.down`/`object.downArrow` (`J`, ↓) and
`object.up`/`object.upArrow` (`K`, ↑) move the focus of a Document column through `focusOrder(document,
collapsed)` (`lib/document.ts`: the values in view order, collapsed groups and the repeating `events` left out);
`object.left`/`object.right` and their arrows move the active column; `object.drill` (Enter) opens the focused
value (`openValueAt`: one reference target or another row's key as the next column, the own key toggles the
view, serialized data as a value column); `columns.close` (Escape, scope `global`) closes the rightmost column.
They act only while the keyboard focus is in `.stack` or on nothing, never in inputs; the arrows and Enter leave
a focused grid alone, Enter a focused button, Escape an open menu.

### Addresses and history

Every view has an address (`databee://<slug>/<database>/<table>/<key>?…`, [architecture](architecture.md#addresses)).
`lib/address.ts` ports Rust's parser and formatter (`parseAddress`, which throws an `AddressError`,
`formatAddress`, `sameAddress`, `encodePart`), because addresses are built on every navigation; a Vitest
test checks it against `src/test/fixtures/addresses.json`, which a Rust test writes.

`lib/values.ts` turns a cell into a `Value` (kind and provenance): `valueAt(page, row, col)` for a browse
page (sourced from `page.table` and `page.keys[row]`), `resultValueAt(result, row, col)` for query
results. It reads the result's `kinds` and `serialized` cells and mirrors Rust's `value::value_at`
([architecture](architecture.md#values-and-the-lens)).

### Project tabs

A `ProjectTab` (`components/project/`, mock `Project`) is the Project page of one database
([presets.md](presets.md#the-project-page)); one per session and database. `openProject({ sessionId?, database?,
form? })` (`project` slice) reuses it and replaces its `form` when one is given; `project.open` (global, no key),
the status bar's `presets …` button (`rulesInfo`, loaded per `dataVersion`) and `openAddress` of a `?project`
address call it. It has no history (`nav` stays empty).

- **Loading:** `loadProjectRules(sessionId, database)` keeps `api.projectRules` per `dataVersion` in
  `projectRules`; `changeProject(sessionId, database, change)` calls `api.projectUpdate`, then `bumpData`, so
  documents and grids reload with the new rules, and returns the `AppError` instead of showing it.
- **Sections:** `PresetsSection`, the open form (`ReferenceForm`, `KindForm`, `StyleForm`), `ReferencesSection`
  (coverage per row in component state: `api.referenceCoverage` with the row's `reference`), `StylesSection`,
  `KindsSection`. They share `ProjectContext` (`context.ts`) and the `:is(.object, .project)` band and row classes.
- **Forms** (`lib/ruleForms.ts`): form state lives in the tab, so it survives switching tabs. `referenceRule`
  maps the real tables of the pickers to rule names through `ProjectRules.tables`; `fieldError` splits Rust's
  `` `field`: message `` and maps the field to the form field it shows under. `ReferenceForm` previews coverage
  with `api.coveragePreview` 400 ms after the last change.
- **Export:** `api.chooseSavePath(<slug>-<database>.toml, "toml")`, then `api.projectExportTo(…, path)`.

- **Tab addresses** (`slices/nav.ts`): a `TableTab`'s `address` is derived from its table and `where` by
  `tableAddress`: one match on the table's single-column primary key (from the `structures` cache) is the
  row (`…/<table>/<key>?view=rows`), anything else the table with its filter (`…/<table>?col=val`, a `null`
  value written as a bare `?col`). The typed SQL filter never enters an address. A `QueryTab`'s address is
  the database home (`homeAddress`), null without a current database. The connection part is the session's
  `slug`.
- **History:** every tab has `nav { back, forward }`, at most `NAV_LIMIT` (50) addresses each. `showRows`
  (opening a row in place, clearing the column filter) and a replaced preview tab push the address being
  left and clear `forward`; an unchanged address pushes nothing. `navBack` / `navForward` (commands
  `nav.back` ⌘[ and `nav.forward` ⌘], global scope) move the active tab and restore its table and `where`;
  a row address needs the primary key, so they load the structure first. They are enabled only while
  `selectCanNav` finds history, so ⌘[ still outdents in the editor. In WordPress mode both delegate to
  `wpNav` (`goBack` / `goForward`) until WordPress mode is removed. An object tab restores the stack its
  target address had ([Column stack](#column-stack)).
- **`openAddress(text, { preview? })`:** parses, finds the open session by slug, opens a row address as an
  object tab (`openObject`), a table address in a table tab (`openTableTab` with the matching `where`), in
  the preview tab when asked, and `?project` as the Project page (`openProject`). Other targets (home, saved
  query, find) answer with a toast until their views exist. The dev hook reaches it as
  `window.databee.getState().openAddress("databee://…")`.
- **Slug renames:** `renameConnectionSlug(identity, slug)` calls Rust, then rewrites `slug` on the sessions
  and the connection part of every address of their tabs. Settings → Connections lists the saved
  connections with their slugs (`components/settings/ConnectionSlugs.tsx`, anchors `settings:slug:<identity>`).

The Sidebar's "New query: SELECT …" asks Rust for the statement (`api.tableSelectSql`), so identifier
quoting stays in Rust too.

Interactive elements carry `data-stage`, `data-stage-label` and `data-stage-list` attributes. These are the
anchors for the [Stage hook](stage.md). Keep them stable when you change components.

`src/lib/` holds pure helpers, each with a test next to it:

| File | Contents |
|---|---|
| `format.ts` | bytes, counts (`formatCount`, `compactCount`), cell text (`cellDisplay` only for `Value`), TSV/JSON export, `relativeTime` |
| `values.ts` | `valueAt`, `resultValueAt`, `cellValue`, `utcSeconds` (see [Addresses and history](#addresses-and-history)) |
| `ruleForms.ts` | the Project page's form state (`ReferenceForm`, `KindForm`, `StyleForm`), prefills from a `RuleDraft` or a listed rule, `referenceRule` / `kindRule` / `styleRule`, `fieldError`, display texts (see [Project tabs](#project-tabs)) |
| `provenance.ts` | `provenanceText(value)` and `sourceText(provenance)`: the status bar text of a value |
| `cells.ts` | `CellSource` and `fullCellText`: a cell's full text, fetched for `longText` |
| `clipboard.ts` | `copyText` with a success or error toast |
| `address.ts` | `parseAddress` / `formatAddress`, the port of Rust's `address` module |
| `sql.ts` | statement ranges and the statement at the cursor |
| `theme.ts` | `applyAppearance` (scheme, accent, `--pad`, sizes, font families), `resolveScheme` / `useResolvedScheme`, `useSystemDark` (one shared `prefers-color-scheme` query), `useRowHeight` (`--row-h` measured once per appearance change), `applyInitialScheme` (first paint) |
| `codeTheme.ts` | CodeMirror theme and syntax colors from the scheme's `--code-*` tokens |
| `document.ts` | object addresses (`objectAddress`, `documentText`, `objectTitle`), `findValue` / `provenanceKey`, group texts (`relationText`, `originText`, `countText`, `moreText`), `groupAnchors`, `collapseKey`, `focusOrder` |
| `columns.ts` | the column stack's types (`ObjectColumn`, `ValueColumn`, `StackColumn`, `ColumnStack`), `objectColumn`, `objectColumnAt`, `columnName` / `columnLabel`, `shownColumns` (see [Column stack](#column-stack)) |
| `crumbs.ts` | WordPress breadcrumbs |
| `wpFormat.ts` | WordPress formatting (absolute times, flags, initials, `shorten`) |

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
- **Derived tokens.** `theme.css` derives the opacity levels (`--o2`, `--o3`), the accent mixes (`--cell`,
  `--sel`, `--lit`), `--accent` from `html[data-accent]` (default orange), the type sizes (`--fs-*`) and the
  row height `--row-h` from `--pad`. Old names that the WordPress screens still read remain as aliases
  marked `/* alias: removed in U2b/U12 */`; core CSS and components use only the new names.
- **Sizing.** Everything is in rem. `html { font-size: var(--ui-font-size) }`, so ⌘= and ⌘- scale the whole UI.
  `--data-font-size` sizes the grid, the editor and the value trees. The row-padding setting (steps 1 to 4)
  sets `--pad` (1, 3, 5 or 8px); `--row-h` is a 20px line (more for large data sizes) plus twice `--pad`.
  `useRowHeight` hands the measured value to the virtualizer.
- **Fonts.** Geist (400, 500, 600) and JetBrains Mono (400, 500, 600, italic 400) are bundled as woff2 in
  `src/assets/fonts/` with their OFL license files. The `fontUi` / `fontMono` settings put an installed family
  in front of the bundled stack (`--font-ui-bundled`, `--font-mono-bundled`).
- **CodeMirror.** `QueryEditor` passes `theme="none"` and adds `codeTheme(dark)`, so the editor reads the
  `--code-*` tokens; `dark` (from `useResolvedScheme`) only picks CodeMirror's base styles.
- **macOS look.**
  - The toolbar leaves `padding-left` for the traffic lights.
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
  `src/test/settings.ts` provides default settings, and `src/test/wpFixtures.ts` the WordPress data.
- **Style.** Query by role or text, drive the UI with `userEvent`, and assert what is visible or which API
  call was made. Reset with `resetStore()` or `connectedStore()` in `beforeEach`.
