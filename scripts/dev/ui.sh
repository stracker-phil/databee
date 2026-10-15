#!/bin/zsh
# ui.sh <x> <y> [text] [enter] — click at pixel coords of a screenshot.sh PNG (Retina 2x),
# optionally type text and press return. Needs Accessibility permission for the terminal.
B=$(swift -e 'import CoreGraphics; let l = CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as! [[String: Any]]; for w in l where (w["kCGWindowOwnerName"] as? String) == "databee" && (w["kCGWindowLayer"] as? Int) == 0 { let b = w["kCGWindowBounds"] as! [String: Double]; print(Int(b["X"]!), Int(b["Y"]!)); break }' 2>/dev/null)
X=$(( ${B% *} + $1 / 2 )); Y=$(( ${B#* } + $2 / 2 ))
osascript <<OSA >/dev/null
tell application "System Events"
  set frontmost of (first process whose name is "databee") to true
  delay 0.3
  click at {$X, $Y}
  delay 0.3
  if "$3" is not "" then keystroke "$3"
  if "$4" is not "" then key code 36
end tell
OSA
