#!/bin/zsh
# Installs the release build as the one DataBee of this machine: ~/Applications/DataBee.app links to it and
# the `databee` command links through that app. A running DataBee (any build) restarts on it.
# `bun run release` ends here; debug builds are never installed (scripts/dev/launch.sh runs them by path).
set -e
ROOT=$(cd "$(dirname "$0")/.." && pwd)
APP="$ROOT/src-tauri/target/release/bundle/macos/DataBee.app"
[[ -d "$APP" ]] || { echo "Not found: $APP (run 'bun run release')"; exit 1; }
LINK="$HOME/Applications/DataBee.app"

running() { pgrep -lf '\.app/Contents/MacOS/databee( |$)' | grep -vq ' mcp$'; }
# Re-signing a running app's files can get it killed, so it quits first.
WAS_RUNNING=
if running; then
  WAS_RUNNING=1
  "$ROOT/scripts/stop-app.sh" "$APP/Contents/MacOS/databee"
fi
"$ROOT/scripts/sign-app.sh" "$APP" com.philippstracker.databee

mkdir -p "$HOME/Applications" "$HOME/.local/bin"
ln -sfn "$APP" "$LINK"
ln -sf "$LINK/Contents/MacOS/databee" "$HOME/.local/bin/databee"
echo "Installed the release build as ~/Applications/DataBee.app and ~/.local/bin/databee"

[[ -n "$WAS_RUNNING" ]] || exit 0
# Launch Services can refuse right after the old app quit (error -600), so try a few times.
for _ in {1..10}; do
  open -g "$LINK" 2>/dev/null || true
  sleep 1
  running && { echo "Restarted DataBee on the new release build"; exit 0; }
done
echo "DataBee did not start again; open it from ~/Applications" >&2
exit 1
