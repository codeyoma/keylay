#!/bin/bash
# Builds KeyLay, signs it with the Developer ID in your keychain, has Apple notarize it, and
# installs it to /Applications from the notarized disk image, the same way a download would be.
#
# The signature matters: macOS remembers permissions such as Bluetooth per signing identity.
# An unsigned (ad-hoc) build looks like a new app every time, so macOS asks again after
# every install. Notarization lets Gatekeeper open the app without a warning, here and on
# other Macs.
#
# One-time setup, notarization credentials in the keychain (an app-specific password from
# account.apple.com, or an App Store Connect API key):
#   xcrun notarytool store-credentials keylay-notary --apple-id <Apple ID> --team-id <Team ID>
#
# Environment:
#   APPLE_SIGNING_IDENTITY  signing identity (default: the first Developer ID Application)
#   NOTARY_PROFILE          notarytool keychain profile (default: keylay-notary)
#   SKIP_NOTARIZE=1         sign and install without notarizing
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
profile=${NOTARY_PROFILE:-keylay-notary}
notarize=1
if [ "${SKIP_NOTARIZE:-}" = 1 ]; then
  notarize=0
elif ! xcrun notarytool history --keychain-profile "$profile" >/dev/null 2>&1; then
  echo "No notarization credentials in keychain profile '$profile'. Run once:" >&2
  echo "  xcrun notarytool store-credentials $profile --apple-id <Apple ID> --team-id <Team ID>" >&2
  echo "or set SKIP_NOTARIZE=1 to install without notarizing." >&2
  exit 1
fi
echo "Signing with: $APPLE_SIGNING_IDENTITY"

# Tauri signs the app; the disk image wraps it
npx --yes @tauri-apps/cli@2 build --bundles app,dmg
app=target/release/bundle/macos/KeyLay.app
dmg=$(ls -t target/release/bundle/dmg/*.dmg | head -1)
codesign --verify --strict "$app"
codesign --force --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$dmg"

if [ "$notarize" = 1 ]; then
  echo "Notarizing $(basename "$dmg") (usually a few minutes)..."
  xcrun notarytool submit "$dmg" --keychain-profile "$profile" --wait
  xcrun stapler staple "$dmg"
  spctl --assess --type open --context context:primary-signature -v "$dmg"
fi

# Install from the disk image, like a download
mount=$(mktemp -d)
hdiutil attach "$dmg" -nobrowse -readonly -mountpoint "$mount" >/dev/null
trap 'hdiutil detach "$mount" -quiet || true' EXIT
pkill -x keylay || true
rm -rf /Applications/KeyLay.app
ditto "$mount/KeyLay.app" /Applications/KeyLay.app
hdiutil detach "$mount" -quiet
trap - EXIT
if [ "$notarize" = 1 ]; then
  spctl --assess -v /Applications/KeyLay.app
fi
open /Applications/KeyLay.app
echo "Installed /Applications/KeyLay.app from $dmg"
