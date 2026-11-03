#!/bin/zsh
# Captures the debug app's window, also in the background: screenshot.sh <out.png>
# Picks its largest normal window so sheets don't hide the app; the release app is never captured.
WID=$(swift - <<'SWIFT' 2>/dev/null
import CoreGraphics
let all = CGWindowListCopyWindowInfo(.optionAll, kCGNullWindowID) as! [[String: Any]]
// macOS reports a window on another Space as not on screen; then any window of the app counts.
let own = all.filter { ($0["kCGWindowOwnerName"] as? String) == "DataBee (Debug)" && ($0["kCGWindowLayer"] as? Int) == 0 }
let onscreen = own.filter { ($0["kCGWindowIsOnscreen"] as? Bool) == true }
var best: (id: Int, area: Double) = (0, 0)
for w in (onscreen.isEmpty ? own : onscreen) {
  let b = w["kCGWindowBounds"] as! [String: Double]
  let area = b["Width"]! * b["Height"]!
  if area > best.area { best = (w["kCGWindowNumber"] as! Int, area) }
}
print(best.id)
SWIFT
)
[[ -n "$WID" && "$WID" != 0 ]] || { echo "No window of the debug app to capture" >&2; exit 1; }
screencapture -x -o -l "$WID" "$1"
