#!/bin/zsh
# Captures the databee window: screenshot.sh <out.png>
WID=$(swift -e 'import CoreGraphics; let l = CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as! [[String: Any]]; for w in l where (w["kCGWindowOwnerName"] as? String) == "databee" && (w["kCGWindowLayer"] as? Int) == 0 { print(w["kCGWindowNumber"]!); break }' 2>/dev/null)
screencapture -x -o -l "$WID" "$1"
