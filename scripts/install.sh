#!/bin/zsh
# Installs the release build as the one DataBee of this machine: ~/Applications/DataBee.app links to it and
# the `databee` command links through that app. A running release app restarts on it, its window shown or hidden
# as before; a debug app keeps running.
# scripts/release.sh runs this before it commits and tags the version; debug builds are never installed
# (scripts/dev/launch.sh runs them by path).
# Usage: scripts/install.sh [--replace-cli]   (--replace-cli replaces a `databee` command that is not this link)
set -e
ROOT=$(cd "$(dirname "$0")/.." && pwd)
APP="$ROOT/src-tauri/target/release/bundle/macos/DataBee.app"
[[ -d "$APP" ]] || { echo "Not found: $APP (run 'bun run release patch')"; exit 1; }
LINK="$HOME/Applications/DataBee.app"

running() { pgrep -lf 'DataBee\.app/Contents/MacOS/databee( |$)' | grep -vq ' mcp$'; }
# Re-signing a running app's files can get it killed, so it quits first; a hidden window stays hidden.
WAS_RUNNING=
OPEN_ARGS=()
if running; then
  WAS_RUNNING=1
  "$APP/Contents/MacOS/databee" status --json 2>/dev/null | grep -q '"windowVisible": *false' \
    && OPEN_ARGS=(--args --hidden)
  "$ROOT/scripts/stop-app.sh" "$APP/Contents/MacOS/databee"
fi
"$ROOT/scripts/sign-app.sh" "$APP" com.philippstracker.databee

mkdir -p "$HOME/Applications" "$HOME/.local/bin"
ln -sfn "$APP" "$LINK"
CLI="$HOME/.local/bin/databee"
TARGET="$LINK/Contents/MacOS/databee"
if [[ ! -e "$CLI" && ! -L "$CLI" ]] || [[ "$(readlink "$CLI")" == "$TARGET" ]] || [[ "$1" == "--replace-cli" ]]; then
  ln -sfn "$TARGET" "$CLI"
  echo "Installed the release build as ~/Applications/DataBee.app and ~/.local/bin/databee"
else
  echo "Installed the release build as ~/Applications/DataBee.app"
  echo "Left ~/.local/bin/databee alone: it is not this script's link (run with --replace-cli to replace it)"
fi

[[ -n "$WAS_RUNNING" ]] || exit 0
# Launch Services can refuse right after the old app quit (error -600), so try a few times.
for _ in {1..10}; do
  open -g "$LINK" "${OPEN_ARGS[@]}" 2>/dev/null || true
  sleep 1
  running && { echo "Restarted DataBee on the new release build"; exit 0; }
done
echo "DataBee did not start again; open it from ~/Applications" >&2
exit 1
