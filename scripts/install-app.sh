#!/bin/zsh
# Makes DataBee available in Spotlight/Launchpad by linking the built app into ~/Applications.
# The link points at the build output, so every `bun run build:app` is picked up automatically.
# Usage: scripts/install-app.sh [path/to/DataBee.app]   (default: the debug build)
set -e
APP="${1:-$(cd "$(dirname "$0")/.." && pwd)/src-tauri/target/debug/bundle/macos/DataBee.app}"
[[ -d "$APP" ]] || { echo "Not found: $APP (run 'bun run build:app' first)"; exit 1; }
mkdir -p "$HOME/Applications"
ln -sfn "$APP" "$HOME/Applications/DataBee.app"
echo "Linked ~/Applications/DataBee.app -> $APP"
