# Frontend

`src/` holds a React 19 + TypeScript app, bundled by Vite. State lives in one zustand store. The frontend
renders and forwards intent; [Rust owns the logic](architecture.md#design-rules).

## Boot

1. `index.html` loads `main.tsx`.
2. `main.tsx` exposes the store as `window.databee` for the dev scripts. It renders
   `<ErrorBoundary><App/></ErrorBoundary>`.
3. `App.tsx` loads the two stylesheets and calls `useBackendEvents()` once:
   1. `installCommands()` registers the command registry.
   2. `loadSettings()` loads settings and keybindings. Commands are re-installed with the user's bindings,
      then `drainLaunches()` pulls the queued launch requests (`take_launch_events`).
   3. It subscribes to `launch` (drain again), `command` (menu clicks → `runCommand`) and the Stage events
      (`installStageHooks()`).
   4. It adds a capture-phase `keydown` → `handleKeydown`, plus `error` / `unhandledrejection` → error toast.
4. `applyAppearance(settings)` runs whenever settings change.

Layout:
- Without a session, the window shows `ConnectScreen`.
- With sessions, it shows `Toolbar`, `FavoritesBar` (Database mode), `WpWorkspace` or `DbWorkspace`,
  `PendingTray` and `StatusBar`.
- `Dialogs`, `CommandPalette` and `Toasts` always render on top.

```
App
├─ ConnectScreen                       (no session)
└─ .app grid
   ├─ Toolbar                          connection pill, database select, mode toggle
   ├─ FavoritesBar                     (Database mode)
   ├─ WpWorkspace                      (WordPress mode, see wordpress.md)
   │  or DbWorkspace: Sidebar | TabBar + TabPanels [| Splitter | Inspector]
   │       TabPanels → QueryTab (QueryEditor, ResultsGrid | ErrorPanel)
   │                 → TableTab (filter bar + ResultsGrid | StructureView)
   ├─ PendingTray
   └─ StatusBar
Dialogs · CommandPalette · Toasts
```

## Store

`src/state/store.ts` is a single store with no middleware.

| Key | Meaning |
|---|---|
| `settings` | `Settings \| null`; null until loaded |
| `sessions`, `activeSessionId` | open connections (`SessionInfo`) |
| `tabs`, `activeTabId` | `QueryTab` (sql, running, response, error) or `TableTab` (database, table, data/structure view, filter, preview) |
| `schema` | per session: databases and tables per database |
| `structures` | `describeTable` replies by `session/database/table`, tagged with the `dataVersion` they belong to (`loadStructure`, `selectStructure`) |
| `mode`, `modeChosen` | `"wp" \| "db"`. A WordPress database switches to `wp` until the user picks a mode |
| `wp`, `wpNav` | detection results per `session/db`; navigation stack (max 100) |
| `dialog` | one dialog at a time, as a discriminated union (settings, connect, password, confirm, prompt, insertRow) |
| `pending`, `applied`, `applying` | plans Rust keeps (`PlannedChange`: id, description, statements for display, `canUndo`); the last 20 applied batch ids, for undo; an apply or undo is in flight |
| `selectedCell` | `{ tabId, row, col, column, cell }`; opens the Inspector |
| `queryCounter` | query tabs opened per session, for unique default titles (`Query 3`) |
| `dataVersion` | bumped by ⌘R and after apply or undo; views re-fetch when it changes |
| `historyVersion` | bumped when `runQuery` finishes; the history panel reloads on change |
| `trayFocusRequest` | bumped by `focusPendingTray()`; `PendingTray` focuses its Apply button (the status bar chip uses it) |
| `paletteOpen`, `keybindings`, `favorites`, `toasts`, `uiState`, `recentNav`, `entityMeta` | as named |

Conventions:
- Components subscribe with narrow selectors (`useStore((s) => s.x)`), and `useShallow` for derived arrays.
- Event handlers call actions through `useStore.getState()`.
- Selectors live at the bottom of the file: `selectActiveSession`, `selectActiveTab`, `selectWp`,
  `selectWpRoute`. For `selectWp`, `undefined` means not probed yet and `null` means not WordPress.
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
- Writes: `queueChange` asks Rust to plan (`planChange` for row edits, `wpPlanChange` for WordPress edits) and
  keeps the returned plan; `applyChanges` sends the active session's plan ids, `undoLast` the newest batch id
  (`undoBatch`); `discardChange` / `discardAll` drop plans in Rust too. The page never sends SQL to write.
- In-flight guard: `applyChanges` and `undoLast` return early while `applying` is set; they set it before
  the first `await` and clear it in `finally`. `PendingTray` disables Apply and Undo meanwhile. `runQuery`
  refuses a tab that is already `running`.
- Request tokens (`beginRequest(key)` / `isLatest(key, token)`): only the newest request per key writes its
  reply. `loadFavorites` uses one key for all sessions; `loadWp` uses `wp:<session>/<database>` and switches
  to WordPress mode only while that session and database are still active.
- Selection: `selectCell` tags the cell with the active tab. `clearSelectionFor(state, tabId)` drops it when
  its tab closes or is left (`activate`); a `dataVersion` bump (`bumpData`) clears it outright.
- `disconnect(sessionId)` purges in one `set`: the session, its tabs, schema, `queryCounter`, `pending` and
  `applied` entries, every `wp` and `structures` key `<session>/…`, the selection if its tab was closed, and `wpNav` and
  `entityMeta` when the session was active.
- `switchDatabase` resets `wpNav`, the selection and `entityMeta`; unless the target is known WordPress it
  also resets `mode` to `db` and `modeChosen`. `connect` resets `wpNav`, `mode` and the selection and reuses
  the session's query tab on reconnect.
- Tab updates are typed: `updateQueryTab` and `updateTableTab` only patch tabs of their kind.
- UI state (`setTableWidths`, `setPaneSize`) is written debounced; `flushUiState` writes it on
  `beforeunload` (registered in `App.tsx`).
- Some errors become state instead: a password dialog, a tab's `ErrorPanel`, a confirm dialog for
  `confirmation_required`.

## API boundary

`src/api/index.ts` exports `api`, a flat object of thin `invoke` wrappers with no logic, plus the event
subscriptions `onLaunch`, `onCommand` and `onStage*`. `src/api/types.ts` mirrors the serialized Rust types.
Keep both in sync with Rust by hand; the roadmap mentions generating them. `toAppError(e)` normalizes
rejections to `AppError`. Derived connection facts come from Rust and are never recomputed in the page: the
Keychain `account` (which also scopes favorites and history) is on `SessionInfo`, `RecentConnection` and login
errors (`AppError.account`).

## Commands, shortcuts, palette, menu

Every user action that has a shortcut or appears in the palette is a `Command` (`src/commands/registry.ts`):

```ts
{ id, title, scope, defaultBinding?, when?, run, menu? }
```

- **Bindings** are strings like `Mod+Shift+K`, where `Mod` means ⌘. They are normalized to the order
  `Mod, Ctrl, Alt, Shift` and shown as `⌘⇧K`. A trailing `++` is the `+` key (`Mod+Shift++`).
- **Scopes** form a tree: `global` → `wp` → `document`, and `global` → `db` → `table` / `editor`.
  `activeScopes()` derives the current ones from the store. When several commands match a key, the most
  specific active scope wins.
- **Keys.** `handleKeydown` runs in the capture phase, so app shortcuts beat CodeMirror and inputs. Inside text
  inputs, only `Mod`/`Ctrl` combinations and Escape fire. While a dialog or the palette is open, only the
  commands in `MODAL_ALLOWED` (Settings toggle, zoom) run; other keys reach the overlay. Native menu items
  go through `runMenuCommand`, which applies the same rule. Auto-repeated keydowns are swallowed unless the
  command is in `REPEATABLE` (zoom, back/forward, next/previous tab). Keys inside a `[data-recording]`
  element (the Settings → Shortcuts recorder) are left alone, so already-bound combos can be recorded.
- **Key names.** `bindingFromEvent` uses `e.key`, so punctuation follows the user's layout. With ⌥ held,
  for dead keys and for non-Latin characters it uses the physical key from `e.code` instead (`KeyK` → `K`,
  `Digit1` → `1`, `Minus` → `-`), so ⌥⌘K records as `Mod+Alt+K`.
- **Overrides** come from `keybindings.json` (`null` unbinds). Settings → Shortcuts records new bindings and
  flags conflicts in overlapping scopes.
- **Palette** (`CommandPalette.tsx`) lists the commands whose `when()` passes, plus tables, entities and
  recent items. The prefixes `>`, `t:`, `#` and `@` narrow the search. `usePaletteItems(query)` builds
  the results and runs the entity search; `PaletteList` renders them and scrolls the active one into
  view. Enter opens a table in the preview tab, ⌘↵ (`goAlt`) opens it pinned.
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
| Shell | `Toolbar`, `FavoritesBar`, `StatusBar`, `PendingTray`, `Splitter` (resizable panes, persisted per connection), `ErrorBoundary` |
| Connect | `ConnectScreen` / `ConnectForm`: TCP or socket, recent connections |
| Database workspace | `Sidebar` (tables tree with filter and arrow-key navigation over a roving `tabIndex`, history reloaded on `historyVersion`), `Tabs`, `QueryTab`, `QueryEditor` (CodeMirror, MySQL dialect, schema completion), `ResultsGrid` (virtualized, resizable, sortable, keyboard navigation, copy as TSV/JSON, inline edit), `TableTab` (paged browse, WHERE/id filter, structure view), `Inspector` + `ValueTree` (PHP/JSON decode) |
| Overlays | `Modal`, `Dialogs` (the insert dialog sets each column to a value, NULL or its default; default leaves it out of the INSERT), `SettingsDialog` (pages: Appearance, Shortcuts, Queries, Display, Connections, WordPress, Presentations, Logging), `CommandPalette`, `ContextMenu`, `Toasts` |
| WordPress | `components/wp/`, see [wordpress.md](wordpress.md#screens) |

### Workspace layout

`App.tsx`: `Workspace` returns the connect screen, the WordPress shell or the database shell (`AppShell`
wraps the toolbar, pending tray and status bar). In `DbWorkspace`, `main` (`TabBar` + `TabPanels`) keeps
the same tree position whatever the inspector does: docking only switches the class on `.workspace`
(`dock-right`, `dock-bottom`) and the `--inspector-size` variable, so CodeMirror history and table pages
survive. The sidebar width is the `--sidebar-width` token in `theme.css`. All tabs of the active session
stay mounted; inactive panels are `hidden`. Tabs use a roving `tabIndex` with Left/Right arrows
(`TabItem`, `tabIcon`). Shortcut hints (new tab, run statement, run all, editor placeholder) come from
`useBindingLabel(commandId)` in `commands/index.ts`, so they follow rebinding.

### Results grid

`ResultsGrid` resets its selection and inline editor when `rows` changes identity and clamps indices
before use. Keys live in `KEY_ACTIONS` (arrows, Home/End, PageUp/PageDown, Enter to edit, ⌘C). `bytes`
cells are never editable: they show a `0x…` preview, which an edit would write back.

### Table tabs

`TableTab.tsx`: `useTableStructure` reads the store's `structures` cache (`loadStructure`, keyed by
`session/database/table`, one `describeTable` per `dataVersion`, purged on disconnect); `DataView` and
`StructureView` share it. `DataView` browses only once the structure (or its error) is known, so an ID
filter always has the primary key. `useBrowse` drops replies that are not the latest request, and the
`Pager` moves by `BrowsePage.limit` (Rust's clamped page size). A failed structure shows an alert and
leaves editing off. `FilterBar` and `useRowEditing` hold the filter input and the row menu and cell edits;
primary-key and WordPress id columns match case-insensitively.

Interactive elements carry `data-stage`, `data-stage-label` and `data-stage-list` attributes. These are the
anchors for the [Stage hook](stage.md). Keep them stable when you change components.

`src/lib/` holds pure helpers, each with a test next to it:

| File | Contents |
|---|---|
| `format.ts` | bytes, counts (`formatCount`, `compactCount`), cell text, TSV/JSON export, `relativeTime` |
| `clipboard.ts` | `copyText` with a success or error toast |
| `sql.ts` | statement ranges and the statement at the cursor, `quoteIdent`, `sqlString`, `whereEquals` |
| `theme.ts` | appearance, row height, `useResolvedTheme` (one shared `prefers-color-scheme` query) |
| `crumbs.ts` | WordPress breadcrumbs |
| `wpFormat.ts` | WordPress formatting (absolute times, flags, initials, `shorten`) |
| `stage.ts` | rectangle math |

## Styling

The CSS is plain and global: `styles/theme.css` holds the tokens and `styles/app.css` the components,
sectioned with `/* ---------- name ---------- */` comments. There are no CSS modules. Icons come from
`lucide-react`.

- **Inset canvas.** The chrome (toolbar, sidebar, tab bar, status bar) is transparent over the window's native
  vibrancy. Content sits on raised panels (`--bg-panel`, `--radius-panel`, `--shadow-panel`), with tonal cards
  (`--bg-surface`) inside.
- **Color roles.**
  - `--accent` is for interactive elements only. The choices are blue, purple, pink and graphite, set through
    `html[data-accent]`.
  - Status colors (`--success`, `--warning`, `--danger`, `--info`, each with a `-soft` variant) are only for
    status.
  - Charts use `--chart-*` and `--category-*`. Grid cells are tinted by type with `--type-*`.
- **Dark mode.** `html[data-theme]` is `light` or `dark`; without the attribute, the system theme applies.
  The dark tokens are defined twice, once in the `prefers-color-scheme` media query and once under
  `[data-theme="dark"]`. Edit both. Rust sets the native window theme so the vibrancy matches.
- **Sizing.** Everything is in rem. `html { font-size: var(--ui-font-size) }`, so ⌘= and ⌘- scale the whole UI.
  `--data-font-size` sizes the grid, the editor and the value trees. Density changes only `--row-height`,
  which the CSS and the virtualizer share.
- **macOS look.**
  - The toolbar leaves `padding-left` for the traffic lights.
  - `data-tauri-drag-region` marks the draggable areas.
  - Fonts are SF Pro and SF Mono.
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
