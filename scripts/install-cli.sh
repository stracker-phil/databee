#!/bin/zsh
# Links the `databee` command to the built app so scripts can launch it.
# Usage: scripts/install-cli.sh [path/to/DataBee.app]   (default: the debug build)
set -e
APP="${1:-$(cd "$(dirname "$0")/.." && pwd)/src-tauri/target/debug/bundle/macos/DataBee.app}"
BIN="$APP/Contents/MacOS/databee"
[[ -x "$BIN" ]] || { echo "Not found: $BIN (run 'bun run app' first)"; exit 1; }
mkdir -p "$HOME/.local/bin"
ln -sf "$BIN" "$HOME/.local/bin/databee"
echo "Linked ~/.local/bin/databee -> $BIN"
