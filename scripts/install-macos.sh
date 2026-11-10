#!/bin/bash
# Builds KeyLay, signs it with the Developer ID in your keychain, installs it to /Applications
# and starts it.
#
# The signature matters: macOS remembers permissions such as Bluetooth per signing identity.
# An unsigned (ad-hoc) build looks like a new app every time, so macOS asks again after
# every install. Set APPLE_SIGNING_IDENTITY to pick another identity.
set -euo pipefail
cd "$(dirname "$0")/../src-tauri"

if [ -z "${APPLE_SIGNING_IDENTITY:-}" ]; then
  APPLE_SIGNING_IDENTITY=$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application: .*\)"/\1/p' | head -1)
fi
if [ -z "$APPLE_SIGNING_IDENTITY" ]; then
  echo "No Developer ID Application identity in the keychain; set APPLE_SIGNING_IDENTITY." >&2
  exit 1
fi
export APPLE_SIGNING_IDENTITY
echo "Signing with: $APPLE_SIGNING_IDENTITY"

npx --yes @tauri-apps/cli@2 build --bundles app
app=target/release/bundle/macos/KeyLay.app
codesign --verify --strict "$app"

pkill -x keylay || true
rm -rf /Applications/KeyLay.app
ditto "$app" /Applications/KeyLay.app
open /Applications/KeyLay.app
echo "Installed /Applications/KeyLay.app"
