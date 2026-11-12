#!/bin/zsh
# Captures the running debug app's main window in every color scheme (docs/ux.md → Palette checks):
# screenshot-looks.sh <out-dir> writes <out-dir>/<scheme>.png, then puts the theme and schemes back.
# Each scheme is shown through the matching theme (light schemes as Light, dark ones as Dark).
set -e
DIR=${1:?usage: screenshot-looks.sh <out-dir>}
HERE=${0:A:h}
mkdir -p "$DIR"
SCHEMES=(ayu-light catppuccin-latte github-light solarized-light ayu-mirage monokai nord solarized-dark)
"$HERE/js.sh" "window.__databeeLooks = { ...window.databee.getState().settings }"
for scheme in $SCHEMES; do
  case $scheme in
    ayu-light|catppuccin-latte|github-light|solarized-light) theme=light; slot=lightScheme ;;
    *) theme=dark; slot=darkScheme ;;
  esac
  "$HERE/js.sh" "(() => { const s = window.databee.getState(); s.saveSettings({ ...s.settings, theme: '$theme', $slot: '$scheme' }, { quiet: true }); })()"
  sleep 1
  "$HERE/screenshot.sh" "$DIR/$scheme.png"
done
"$HERE/js.sh" "(() => { const s = window.databee.getState(); s.saveSettings(window.__databeeLooks, { quiet: true }); })()"
echo "Wrote ${#SCHEMES} screenshots to $DIR"
