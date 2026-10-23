#!/bin/zsh
# Signs a built DataBee bundle so macOS keeps its Keychain access to saved passwords across rebuilds: the
# access belongs to the identifier and the signing certificate, not to the build. Uses
# $DATABEE_SIGN_IDENTITY, else the "DataBee Local Signing" certificate of scripts/create-signing-cert.sh,
# else signs ad hoc (the Keychain then asks again after each rebuild).
# Usage: scripts/sign-app.sh path/to/App.app bundle.identifier
set -e
APP=$1 IDENTIFIER=$2
IDENTITY=${DATABEE_SIGN_IDENTITY:-}
if [[ -z "$IDENTITY" ]] && security find-identity -p codesigning | grep -q '"DataBee Local Signing"'; then
  IDENTITY="DataBee Local Signing"
fi
codesign --force --sign "${IDENTITY:--}" --identifier "$IDENTIFIER" "$APP" 2>/dev/null
[[ -n "$IDENTITY" ]] || echo "Signed ad hoc: the Keychain asks again after each rebuild (scripts/create-signing-cert.sh fixes that)"
