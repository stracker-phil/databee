#!/bin/zsh
# Runs JavaScript in the running debug app without focusing it (dev builds only).
# Usage: scripts/dev/js.sh 'document.querySelector("[aria-label=\"Settings\"]").click()'
SOCK="$HOME/Library/Application Support/com.philippstracker.databee/databee.sock"
python3 -I - "$SOCK" "$1" <<'PY'
import json, socket, sys
s = socket.socket(socket.AF_UNIX)
s.connect(sys.argv[1])
s.sendall((json.dumps({"type": "eval", "js": sys.argv[2]}) + "\n").encode())
s.recv(16)
PY
