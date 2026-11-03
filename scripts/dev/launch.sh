#!/bin/zsh
# Restarts the debug app next to the terminal (same screen, free area) without taking focus.
# Usage: scripts/dev/launch.sh [databee CLI args...]     Terminal app name: $DATABEE_DEV_TERMINAL (default Terminal)
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
APP="$ROOT/src-tauri/target/debug/bundle/macos/DataBee (Debug).app"
BIN="$APP/Contents/MacOS/databee"
[[ -x "$BIN" ]] || { echo "Build first: bun run build:app"; exit 1; }
# A failed build leaves the previous bundle behind; never start one older than the code it should run.
STALE=$(find "$ROOT/src" "$ROOT/src-tauri/src" "$ROOT/src-tauri/presets" "$ROOT/src-tauri/capabilities" \
  "$ROOT/src-tauri/build.rs" "$ROOT/src-tauri/Cargo.toml" "$ROOT/src-tauri/tauri.conf.json" "$ROOT/src-tauri/tauri.debug.conf.json" \
  -type f -not -name '*.test.*' -not -name '*.gen.ts' -not -name '.DS_Store' -not -path '*/test/*' -not -name 'test_support.rs' \
  -not -name 'server_tests.rs' -not -path '*/fixtures/*' -newer "$BIN" -print -quit)
[[ -z "$STALE" ]] || { echo "The debug build is older than ${STALE#$ROOT/}; run bun run build:app first"; exit 1; }

"$ROOT/scripts/sign-app.sh" "$APP" com.philippstracker.databee.debug

FRAME=$(swift - "${DATABEE_DEV_TERMINAL:-Terminal}" <<'SWIFT' 2>/dev/null
import AppKit
let owner = CommandLine.arguments[1]
let mainHeight = NSScreen.screens[0].frame.height
// Screens in window coordinates (top-left origin, y down).
let screens = NSScreen.screens.map { s -> CGRect in
  let v = s.visibleFrame
  return CGRect(x: v.minX, y: mainHeight - v.maxY, width: v.width, height: v.height)
}
let windows = CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as! [[String: Any]]
let term = windows.first { ($0["kCGWindowOwnerName"] as? String) == owner && ($0["kCGWindowLayer"] as? Int) == 0 }
var screen = screens[0]
var free = screen
if let t = term, let b = t["kCGWindowBounds"] as? [String: Double] {
  let tr = CGRect(x: b["X"]!, y: b["Y"]!, width: b["Width"]!, height: b["Height"]!)
  screen = screens.first { $0.contains(CGPoint(x: tr.midX, y: tr.midY)) } ?? screens[0]
  let left = tr.minX - screen.minX, right = screen.maxX - tr.maxX
  free = left >= right
    ? CGRect(x: screen.minX, y: screen.minY, width: left, height: screen.height)
    : CGRect(x: tr.maxX, y: screen.minY, width: right, height: screen.height)
  if free.width < 900 { free = screen }
}
let w = min(free.width - 16, 1600), h = min(free.height - 16, 1200)
print("\(Int(free.minX + 8)),\(Int(free.minY + 8)),\(Int(w)),\(Int(h))")
SWIFT
)
# Only the debug app restarts: it has its own data, socket and Stage port, so the release app keeps running beside it.
"$ROOT/scripts/stop-app.sh" "$BIN"
LOG="${TMPDIR:-/tmp}/databee-dev.log"
DATABEE_WINDOW_FRAME="$FRAME" DATABEE_BACKGROUND=1 nohup "$BIN" "$@" >"$LOG" 2>&1 &
disown
echo "DataBee started at $FRAME (log: $LOG)"
