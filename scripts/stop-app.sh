#!/bin/zsh
# Quits the running DataBee app through its socket and waits until it is gone, so it saves its state.
# `running` matches only the app bundle's binary: `databee mcp` and other CLI processes keep running.
# Usage: scripts/stop-app.sh path/to/databee
running() { pgrep -lf '\.app/Contents/MacOS/databee( |$)' | grep -vq ' mcp$'; }
running || exit 0
"$1" quit >/dev/null 2>&1
for _ in {1..50}; do
  running || exit 0
  sleep 0.1
done
echo "DataBee did not quit; quit it from its menu and try again" >&2
exit 1
