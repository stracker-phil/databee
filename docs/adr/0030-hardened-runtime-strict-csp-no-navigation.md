# 0030. Hardened runtime, a strict CSP and no navigation

## Status

Accepted (2026-11-03), implemented. Adopted from Taskboard (its ADR 0049), without `style-src-elem`.

## Context

DataBee renders arbitrary database content: serialized values, HTML in post content, URLs in options. A link
or script that slipped into the page could navigate the webview to another page, which would then talk to the
app's IPC. Builds were signed without the hardened runtime, so libraries could be injected and a debugger
attached.

## Decision

- Builds are signed with the hardened runtime (`codesign --options runtime`) and no entitlements: the
  transparent window (`macos-private-api`), the database client, the Keychain and the MCP server need none.
- The CSP adds `script-src 'self'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'`
  to `default-src 'self'`.
- A navigation guard plugin (`window/navigation.rs`, `on_navigation`) keeps the webview on the app's own
  pages (`tauri://localhost`, `tauri.localhost`, in debug builds the Vite dev server); web links open in the
  browser (`commands::open_url`).

Rejected: `style-src-elem 'self'`. CodeMirror (`style-mod`) and `TableFocus` insert `<style>` elements at
runtime, which it blocks; `EditorView.cspNonce` would need the nonce Tauri adds per load, which the page
cannot read. `style-src` keeps `'unsafe-inline'`, so styles are the remaining injection surface, without
script.

## Consequences

- A navigation away from the app is refused and logged.
- New inline scripts or frames fail in the bundled app; dev hooks evaluate through WebKit, not the page, and
  keep working.
