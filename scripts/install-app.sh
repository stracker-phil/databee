#!/bin/zsh
# Makes databee available in Spotlight/Launchpad by linking the built app into ~/Applications.
# The link points at the build output, so every `bun run build:app` is picked up automatically.
# Usage: scripts/install-app.sh [path/to/databee.app]   (default: the debug build)
set -e
APP="${1:-$(cd "$(dirname "$0")/.." && pwd)/src-tauri/target/debug/bundle/macos/databee.app}"
[[ -d "$APP" ]] || { echo "Not found: $APP (run 'bun run build:app' first)"; exit 1; }
mkdir -p "$HOME/Applications"
ln -sfn "$APP" "$HOME/Applications/databee.app"
echo "Linked ~/Applications/databee.app -> $APP"
