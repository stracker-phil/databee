#!/bin/zsh
# Quits the running DataBee of the build that `path/to/databee` belongs to (release or debug; each has its own
# socket and data) through its socket, and waits until it is gone, so it saves its state. `app_pids`
# matches only that build's app process: `databee mcp` and other CLI processes keep running.
# Usage: scripts/stop-app.sh path/to/databee
source "$(dirname "$0")/app-pids.sh"
running() { app_pids "$1" >/dev/null; }
running || exit 0
"$1" quit >/dev/null 2>&1
for _ in {1..50}; do
  running || exit 0
  sleep 0.1
done
echo "DataBee did not quit; quit it from its menu and try again" >&2
exit 1
