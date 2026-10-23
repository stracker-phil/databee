#!/bin/zsh
# Creates the self-signed "DataBee Local Signing" code-signing certificate in the login keychain, once per
# Mac. scripts/sign-app.sh signs every build with it, so Keychain access to saved passwords survives rebuilds.
set -e
NAME="DataBee Local Signing"
if security find-identity -p codesigning | grep -q "\"$NAME\""; then
  echo "$NAME already exists"
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
security import "$TMP/id.p12" -k "$HOME/Library/Keychains/login.keychain-db" -P "$PASS" -T /usr/bin/codesign
echo "Created $NAME; rebuild, then allow Keychain access once per app (Always Allow)"
