#!/bin/zsh
# Removes build output that only piles up: the app's own debug artifacts (every rebuild of the tests leaves a new
# copy) and the incremental caches (dep-graph.bin and friends). Compiled dependencies stay, so the next build
# takes about a minute, not a full rebuild. Runs at the end of every release; safe to run any time.
set -euo pipefail

ROOT=${0:A:h:h}
TARGET="$ROOT/src-tauri/target"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
[[ -d "$TARGET" ]] || exit 0

before=$(du -sk "$TARGET" | cut -f1)
MANIFEST="$ROOT/src-tauri/Cargo.toml"
PACKAGE=$(sed -n '/^\[package\]/,/^\[/s/^name = "\(.*\)"$/\1/p' "$MANIFEST" | head -1)
cargo clean --manifest-path "$MANIFEST" --package "$PACKAGE" --quiet
rm -rf "$TARGET/debug/incremental" "$TARGET/release/incremental"
after=$(du -sk "$TARGET" | cut -f1)
echo "Cleaned the build folder: $(( before / 1048576 )) GB → $(( after / 1048576 )) GB"
