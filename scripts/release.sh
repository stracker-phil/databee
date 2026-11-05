#!/bin/zsh
# Every release build gets a new version (ADR 0038): release.sh major|minor|patch|X.Y.Z.
# Writes the version into FILES, builds and installs the release app (scripts/install.sh), then commits
# "Release X.Y.Z" and tags vX.Y.Z locally. A failed build puts the version files back, so no version is used
# twice or skipped.
set -e
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
export PATH=/opt/homebrew/opt/rustup/bin:$PATH
FILES=(package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock)

usage() { echo "Usage: bun run release major|minor|patch|X.Y.Z (docs/adr/0038)"; exit 1; }
[[ $# -eq 1 ]] || usage
[[ -z "$(git status --porcelain)" ]] || { echo "Commit or stash the changes first: a release is built from a commit"; exit 1; }

CURRENT=$(sed -n 's/^  "version": "\(.*\)",$/\1/p' package.json)
[[ "$CURRENT" =~ '^[0-9]+\.[0-9]+\.[0-9]+$' ]] || { echo "No X.Y.Z version in package.json"; exit 1; }
IFS=. read -r MAJOR MINOR PATCH <<< "$CURRENT"
case "$1" in
  major) NEXT="$((MAJOR + 1)).0.0" ;;
  minor) NEXT="$MAJOR.$((MINOR + 1)).0" ;;
  patch) NEXT="$MAJOR.$MINOR.$((PATCH + 1))" ;;
  <->.<->.<->) NEXT="$1" ;;
  *) usage ;;
esac
# A version only moves forward.
[[ "$(printf '%s\n%s\n' "$CURRENT" "$NEXT" | sort -t. -k1,1n -k2,2n -k3,3n | tail -1)" == "$NEXT" && "$NEXT" != "$CURRENT" ]] \
  || { echo "$NEXT is not after $CURRENT"; exit 1; }
git rev-parse -q --verify "refs/tags/v$NEXT" >/dev/null && { echo "v$NEXT was released already"; exit 1; }

restore() { git checkout -- "${FILES[@]}"; echo "Release $NEXT failed; the version stays $CURRENT"; }
trap restore ERR
sed -i '' "s/^  \"version\": \"$CURRENT\",$/  \"version\": \"$NEXT\",/" package.json src-tauri/tauri.conf.json
sed -i '' "1,/^version = /s/^version = \"$CURRENT\"$/version = \"$NEXT\"/" src-tauri/Cargo.toml
cargo update --manifest-path src-tauri/Cargo.toml --workspace --offline --quiet
echo "Releasing $CURRENT → $NEXT"

"$ROOT/node_modules/.bin/tauri" build
scripts/install.sh
trap - ERR

git add -- "${FILES[@]}"
git commit -q -m "Release $NEXT"
git tag "v$NEXT"
echo "Released $NEXT (commit and tag v$NEXT, not pushed)"
