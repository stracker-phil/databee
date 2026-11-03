# 0029. Builds are signed with an Apple Development certificate

## Status

Accepted (2026-11-03), implemented. Adopted from Taskboard (its ADR 0053).

## Context

Builds were signed with the self-signed "DataBee Local Signing" certificate. A Keychain item's partition list
admits an app without a Team ID only by its code hash, which changes with every build, so every new release
build asked for the login password before it read a saved password, and "Always Allow" covered one build.
The docs claimed the local certificate kept the trust across rebuilds; it does not.

## Decision

`scripts/sign-app.sh` signs with `$DATABEE_SIGN_IDENTITY`, else the first valid "Apple Development" certificate
(Xcode → Settings → Accounts → Manage Certificates; a free Apple ID is enough), picked by its SHA-1 hash because
an expired certificate may share its name, else the local certificate, else ad hoc. The Apple certificate
carries a Team ID, so the Keychain admits every build of that team after one "Always Allow".
`scripts/create-signing-cert.sh` stays as the fallback and allows codesign to use its key without asking
(`security set-key-partition-list`); a second run repeats that step. The bundler signs ad hoc
(`bundle.macOS.signingIdentity: "-"`); the scripts re-sign.

Rejected: keeping the local certificate and re-adding each build's hash to every item (the password every
time); the data-protection Keychain (needs an entitlement and a provisioning profile); passwords outside the
Keychain ([ADR 0004](0004-passwords-in-secret.md)).

## Consequences

- The first release build after the switch asks once more ("Always Allow"); later builds don't.
- Privacy grants are given once more, since the signature changed.
- No notarization: the app is built and run on the same Mac.
