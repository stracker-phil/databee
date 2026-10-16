#!/bin/zsh
# Captures the DataBee window (works while it is in the background): screenshot.sh <out.png>
# Picks the largest DataBee window so sheets (e.g. Screen Time) don't hide the app.
WID=$(swift -e 'import CoreGraphics; let l = CGWindowListCopyWindowInfo(.optionAll, kCGNullWindowID) as! [[String: Any]]; var best: (Int, Double) = (0, 0); for w in l where (w["kCGWindowOwnerName"] as? String) == "DataBee" && (w["kCGWindowLayer"] as? Int) == 0 && (w["kCGWindowIsOnscreen"] as? Bool) == true { let b = w["kCGWindowBounds"] as! [String: Double]; let area = b["Width"]! * b["Height"]!; if area > best.1 { best = (w["kCGWindowNumber"] as! Int, area) } }; print(best.0)' 2>/dev/null)
screencapture -x -o -l "$WID" "$1"
