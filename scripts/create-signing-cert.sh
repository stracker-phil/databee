#!/bin/zsh
# Creates the self-signed "DataBee Local Signing" code-signing certificate in the login keychain, once per
# Mac, the fallback for a Mac without an Apple Development certificate (ADR 0029). scripts/sign-app.sh signs
# with it then (the Keychain still asks once per new build before it reads a saved password).
# Running it again on a Mac that has the certificate only repeats the last step: allowing codesign to use the
# key without asking, which stops the signing prompt on every build.
set -e
NAME="DataBee Local Signing"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

allow_codesign() {
  echo "Allowing codesign to use the key; enter your login (keychain) password once:"
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -l "$NAME" "$KEYCHAIN" >/dev/null
  echo "Done: builds sign without asking"
}
if security find-identity -p codesigning | grep -q "\"$NAME\""; then
  echo "$NAME already exists"
  allow_codesign
  exit 0
fi
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
cat > "$TMP/ext.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = v3
prompt = no
[dn]
CN = $NAME
[v3]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
CNF
# The system LibreSSL writes a PKCS#12 file that `security import` reads.
/usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -config "$TMP/ext.cnf" \
  -keyout "$TMP/key.pem" -out "$TMP/cert.pem" 2>/dev/null
PASS=$(/usr/bin/openssl rand -hex 16)
/usr/bin/openssl pkcs12 -export -inkey "$TMP/key.pem" -in "$TMP/cert.pem" -name "$NAME" \
  -out "$TMP/id.p12" -passout "pass:$PASS"
security import "$TMP/id.p12" -k "$KEYCHAIN" -P "$PASS" -T /usr/bin/codesign
echo "Created $NAME"
allow_codesign
echo "Rebuild, then allow Keychain access once per build (Always Allow)"
