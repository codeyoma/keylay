#!/bin/bash
# Runs the current code as a debug app bundle (macOS), opens one window and saves a
# screenshot of it.
#
#   scripts/dev-run.sh [popover|settings|none] [output.png]
#   scripts/dev-run.sh stop
#
# Prints the screenshot path. The debug app keeps running in place of any running KeyLay,
# the installed one included; `stop` quits it and starts /Applications/KeyLay.app again.
#
# Why a bundle: the bare binary (target/debug/keylay) has no Info.plist, so macOS kills it
# (SIGABRT, a TCC privacy violation) as soon as it touches Bluetooth. The KEYLAY_SHOW_*
# variables reach the app only through `open --env`.
set -euo pipefail
cd "$(dirname "$0")/../src-tauri"

window=${1:-popover}
if [ "$window" = stop ]; then
  pkill -x keylay || true
  sleep 1
  if [ -d /Applications/KeyLay.app ]; then
    open /Applications/KeyLay.app
    echo "Started /Applications/KeyLay.app"
  fi
  exit 0
fi
out=${2:-${TMPDIR:-/tmp}/keylay-$window.png}
case "$window" in
  popover) env=(--env KEYLAY_SHOW_POPOVER=1) ;;
  settings) env=(--env KEYLAY_SHOW_SETTINGS=1) ;;
  none) env=() ;;
  *) echo "usage: $0 [popover|settings|none] [output.png] | stop" >&2; exit 2 ;;
esac

# cargo lives in the rustup toolchain, which is not always on PATH
if ! command -v cargo >/dev/null; then
  PATH="$(dirname "$(rustup which cargo)"):$PATH"
  export PATH
fi

npx --yes @tauri-apps/cli@2 build --debug --bundles app >/dev/null
app=target/debug/bundle/macos/KeyLay.app

pkill -x keylay || true
sleep 1
open -n "${env[@]}" "$app"
[ "$window" = none ] && { echo "Started $app"; exit 0; }

# Wait for a KeyLay window on screen, then capture it alone (no shadow)
finder=$(mktemp -t keylay-window).swift
cat > "$finder" <<'SWIFT'
import CoreGraphics
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
for w in windows where (w["kCGWindowOwnerName"] as? String ?? "").lowercased() == "keylay" {
  print(w["kCGWindowNumber"]!)
}
SWIFT
id=""
for _ in $(seq 1 15); do
  sleep 1
  id=$(swift "$finder" 2>/dev/null | head -1)
  [ -n "$id" ] && break
done
rm -f "$finder"
if [ -z "$id" ]; then
  echo "No KeyLay window appeared" >&2
  exit 1
fi
sleep 1 # let the page draw
screencapture -x -o -l"$id" "$out"
echo "$out"
