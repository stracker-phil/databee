#!/bin/zsh
# Signs a built DataBee bundle so macOS keeps its Keychain access to saved passwords across rebuilds (ADR 0029).
# Uses $DATABEE_SIGN_IDENTITY, else a valid "Apple Development" certificate (Xcode → Settings → Accounts →
# Manage Certificates; a free Apple ID is enough), else the "DataBee Local Signing" certificate of
# scripts/create-signing-cert.sh, else signs ad hoc.
# Only the Apple certificate carries a Team ID, so the Keychain trusts every build of the team; with the local
# certificate, each new build asks once for the login password before it reads a saved password.
# Signs with the hardened runtime: no injected libraries or debugger attach; the app needs no entitlements
# (not sandboxed; the Keychain, the database client and the MCP server need none).
# Usage: scripts/sign-app.sh path/to/App.app bundle.identifier
set -e
APP=$1 IDENTIFIER=$2
IDENTITY=${DATABEE_SIGN_IDENTITY:-}
if [[ -z "$IDENTITY" ]]; then
  # By hash: an expired certificate of the same name would make the name ambiguous.
  IDENTITY=$(security find-identity -v -p codesigning | awk '/"Apple Development: / { print $2; exit }')
fi
if [[ -z "$IDENTITY" ]] && security find-identity -p codesigning | grep -q '"DataBee Local Signing"'; then
  IDENTITY="DataBee Local Signing"
fi
codesign --force --options runtime --sign "${IDENTITY:--}" --identifier "$IDENTIFIER" "$APP" 2>/dev/null
[[ -n "$IDENTITY" ]] || echo "Signed ad hoc: the Keychain asks again after each rebuild (see scripts/sign-app.sh)"
