#!/bin/sh
# Create a stable, self-signed **code-signing** certificate in your login
# keychain so `bun run tauri build` produces a `coot.app` with a consistent
# code identity. That lets you click "Always Allow" ONCE on the macOS Keychain
# prompt and never be asked again — the authorization is keyed to the signing
# identity, which stays the same across rebuilds (unlike `tauri dev`'s unsigned,
# rebuilt-every-time binary, or ad-hoc signing whose identity changes each build).
#
# Run once:   ./scripts/setup-macos-signing.sh
# It is idempotent (skips if the identity already exists) and only touches YOUR
# login keychain. It will prompt for your login password once (to trust the new
# cert for code signing) — that's macOS, not this script, asking.
#
# Personal/local use only: this is a self-signed cert you trust to sign your own
# app on your own Mac. It is NOT for distribution (no Apple Developer ID / no
# notarization). To undo: delete "coot Local Signing" in Keychain Access.
set -eu

IDENTITY="coot Local Signing"
KEYCHAIN="login.keychain-db"

if security find-identity -v -p codesigning 2>/dev/null | grep -qF "$IDENTITY"; then
  echo "✓ Code-signing identity '$IDENTITY' already exists — nothing to do."
  echo "  Build with:  cd app/ui && bun run tauri build"
  exit 0
fi

echo "Creating self-signed code-signing identity '$IDENTITY'…"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# A minimal cert request with the codeSigning extended key usage (required for a
# code-signing identity).
cat > "$TMP/req.cnf" <<EOF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $IDENTITY
[ext]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
EOF

# Use the system LibreSSL (/usr/bin/openssl) — it emits a PKCS#12 that macOS's
# `security import` reads without the OpenSSL-3 `-legacy` flag dance.
/usr/bin/openssl req -x509 -newkey rsa:2048 -sha256 -nodes -days 3650 \
  -keyout "$TMP/key.pem" -out "$TMP/cert.pem" \
  -config "$TMP/req.cnf" -extensions ext

/usr/bin/openssl pkcs12 -export -out "$TMP/id.p12" \
  -inkey "$TMP/key.pem" -in "$TMP/cert.pem" -name "$IDENTITY" -passout pass:coot

# Import the key+cert into the login keychain; -A lets local tools (codesign)
# use the private key without a per-build access prompt.
security import "$TMP/id.p12" -k "$KEYCHAIN" -P coot -A -T /usr/bin/codesign

# Trust the self-signed cert for the codeSign policy so it validates as a real
# code-signing identity. This is the step that prompts for your login password.
echo "→ macOS will now ask for your login password to trust the certificate…"
security add-trusted-cert -r trustRoot -p codeSign -k "$KEYCHAIN" "$TMP/cert.pem"

if security find-identity -v -p codesigning | grep -qF "$IDENTITY"; then
  echo "✓ Done. '$IDENTITY' is a valid code-signing identity."
  echo
  echo "Next:"
  echo "  1. cd app/ui && bun run tauri build"
  echo "  2. Launch the built app (src-tauri/target/release/bundle/macos/coot.app,"
  echo "     or the .dmg) and click 'Always Allow' on the one Keychain prompt."
  echo "  3. That's it — no more prompts, even after future rebuilds."
else
  echo "⚠ The identity was created but isn't showing as valid for code signing."
  echo "  Fall back to the Keychain Access GUI method in SIGNING.md."
  exit 1
fi
