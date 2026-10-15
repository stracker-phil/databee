#!/bin/zsh
# Captures the databee window (works while it is in the background): screenshot.sh <out.png>
WID=$(swift -e 'import CoreGraphics; let l = CGWindowListCopyWindowInfo(.optionAll, kCGNullWindowID) as! [[String: Any]]; for w in l where (w["kCGWindowOwnerName"] as? String) == "databee" && (w["kCGWindowLayer"] as? Int) == 0 && (w["kCGWindowIsOnscreen"] as? Bool) == true { print(w["kCGWindowNumber"]!); break }' 2>/dev/null)
screencapture -x -o -l "$WID" "$1"
