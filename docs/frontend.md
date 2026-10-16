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
| `mode`, `modeChosen` | `"wp" \| "db"`. A WordPress database switches to `wp` until the user picks a mode |
| `wp`, `wpNav` | detection results per `session/db`; navigation stack (max 100) |
| `dialog` | one dialog at a time, as a discriminated union (settings, connect, password, confirm, prompt, insertRow) |
| `pending`, `applied`, `applying` | planned changes; last 20 applied batches, for undo; an apply or undo is in flight |
| `selectedCell` | `{ tabId, row, col, column, cell }`; opens the Inspector |
| `queryCounter` | query tabs opened per session, for unique default titles (`Query 3`) |
| `dataVersion` | bumped by ⌘R and after apply or undo; views re-fetch when it changes |
| `paletteOpen`, `keybindings`, `favorites`, `toasts`, `uiState`, `recentNav`, `entityMeta` | as named |

Conventions:
- Components subscribe with narrow selectors (`useStore((s) => s.x)`), and `useShallow` for derived arrays.
- Event handlers call actions through `useStore.getState()`.
- Selectors live at the bottom of the file: `selectActiveSession`, `selectActiveTab`, `selectWp`,
  `selectWpRoute`. For `selectWp`, `undefined` means not probed yet and `null` means not WordPress.
- Store actions wrap API errors in `fail(e, context)`, which shows an error toast and writes the message to
  the Rust log.
- In-flight guard: `applyChanges` and `undoLast` return early while `applying` is set; they set it before
  the first `await` and clear it in `finally`. `PendingTray` disables Apply and Undo meanwhile. `runQuery`
  refuses a tab that is already `running`.
- Request tokens (`beginRequest(key)` / `isLatest(key, token)`): only the newest request per key writes its
  reply. `loadFavorites` uses one key for all sessions; `loadWp` uses `wp:<session>/<database>` and switches
  to WordPress mode only while that session and database are still active.
- Selection: `selectCell` tags the cell with the active tab. `clearSelectionFor(state, tabId)` drops it when
  its tab closes or is left (`activate`); a `dataVersion` bump (`bumpData`) clears it outright.
- `disconnect(sessionId)` purges in one `set`: the session, its tabs, schema, `queryCounter`, `pending` and
  `applied` entries, every `wp` key `<session>/…`, the selection if its tab was closed, and `wpNav` and
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
rejections to `AppError`.

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
  inputs, only `Mod`/`Ctrl` combinations and Escape fire.
- **Overrides** come from `keybindings.json` (`null` unbinds). Settings → Shortcuts records new bindings and
  flags conflicts in overlapping scopes.
- **Palette** (`CommandPalette.tsx`) lists the commands whose `when()` passes, plus tables, entities and
  recent items. The prefixes `>`, `t:`, `#` and `@` narrow the search.
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
| Database workspace | `Sidebar` (tables tree with filter, history), `Tabs`, `QueryTab`, `QueryEditor` (CodeMirror, MySQL dialect, schema completion), `ResultsGrid` (virtualized, resizable, sortable, keyboard navigation, copy as TSV/JSON, inline edit), `TableTab` (paged browse, WHERE/id filter, structure view), `Inspector` + `ValueTree` (PHP/JSON decode) |
| Overlays | `Modal`, `Dialogs`, `SettingsDialog` (pages: Appearance, Shortcuts, Queries, Display, Connections, WordPress, Presentations, Logging), `CommandPalette`, `ContextMenu`, `Toasts` |
| WordPress | `components/wp/`, see [wordpress.md](wordpress.md#screens) |

Interactive elements carry `data-stage`, `data-stage-label` and `data-stage-list` attributes. These are the
anchors for the [Stage hook](stage.md). Keep them stable when you change components.

`src/lib/` holds pure helpers, each with a test next to it:

| File | Contents |
|---|---|
| `format.ts` | bytes, counts, cell text, TSV/JSON export, relative time |
| `sql.ts` | statement ranges and the statement at the cursor |
| `theme.ts` | appearance, row height |
| `crumbs.ts` | WordPress breadcrumbs |
| `wpFormat.ts` | WordPress formatting |
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
