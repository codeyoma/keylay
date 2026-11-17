#!/bin/bash
# Runs the current code as a debug app bundle (macOS), opens one window and saves a
# screenshot of it.
#
#   scripts/dev-run.sh [popover|settings|none] [output.png]
#   scripts/dev-run.sh stop
#
# The debug app is "KeyLay Dev" (identifier dev.yoma.keylay.dev): it runs beside the
# installed KeyLay with its own config, window position and cache, so it never touches
# the installed app. Its config starts as a copy of the installed one. `stop` quits it.
#
# Why a bundle: the bare binary (target/debug/keylay) has no Info.plist, so macOS kills it
# (SIGABRT, a TCC privacy violation) as soon as it touches Bluetooth. The KEYLAY_SHOW_*
# variables reach the app only through `open --env`.
set -euo pipefail
cd "$(dirname "$0")/../src-tauri"

identifier=dev.yoma.keylay.dev
app="target/debug/bundle/macos/KeyLay Dev.app"
support="$HOME/Library/Application Support"

# The running debug app, by its executable path (the process is named keylay, like the
# installed one)
dev_pids() { pgrep -f "KeyLay Dev.app/Contents/MacOS/" || true; }

stop_dev() {
  local pids
  pids=$(dev_pids)
  [ -z "$pids" ] && return 0
  # shellcheck disable=SC2086
  kill $pids 2>/dev/null || true
  for _ in $(seq 1 50); do
    [ -z "$(dev_pids)" ] && return 0
    sleep 0.2
  done
  echo "KeyLay Dev did not quit" >&2
  return 1
}

window=${1:-popover}
if [ "$window" = stop ]; then
  stop_dev
  echo "Stopped KeyLay Dev"
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

npx --yes @tauri-apps/cli@2 build --debug --bundles app \
  --config "{\"identifier\":\"$identifier\",\"productName\":\"KeyLay Dev\"}" >/dev/null

# Start from the installed app's keyboards, the first time
if [ ! -f "$support/$identifier/config.json" ] && [ -f "$support/dev.yoma.keylay/config.json" ]; then
  mkdir -p "$support/$identifier"
  cp "$support/dev.yoma.keylay/config.json" "$support/$identifier/config.json"
fi

stop_dev
# ${env[@]+...}: an empty array is "unbound" for set -u in macOS's bash 3.2
open -n ${env[@]+"${env[@]}"} "$app"
[ "$window" = none ] && { echo "Started $app"; exit 0; }

# Wait for a window of the debug app (not its menu bar item; the cheat sheet floats above
# normal windows), then capture it alone
tmp=$(mktemp -d -t keylay-dev)
trap 'rm -rf "$tmp"' EXIT
cat > "$tmp/finder.swift" <<'SWIFT'
import CoreGraphics
let pids = Set(CommandLine.arguments.dropFirst().compactMap { Int($0) })
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
for w in windows {
  guard let pid = w["kCGWindowOwnerPID"] as? Int, pids.contains(pid),
        (w["kCGWindowLayer"] as? Int ?? 0) < 24, // below the menu bar (status items: 25)
        let bounds = w["kCGWindowBounds"] as? [String: Double],
        (bounds["Width"] ?? 0) > 100, (bounds["Height"] ?? 0) > 100 else { continue }
  print(w["kCGWindowNumber"]!)
}
SWIFT
swiftc -O -o "$tmp/finder" "$tmp/finder.swift" 2>/dev/null
id=""
for _ in $(seq 1 30); do
  sleep 0.5
  # shellcheck disable=SC2046
  id=$("$tmp/finder" $(dev_pids) | head -1)
  [ -n "$id" ] && break
done
if [ -z "$id" ]; then
  echo "No KeyLay Dev window appeared" >&2
  exit 1
fi
sleep 1 # let the page draw
screencapture -x -o -l"$id" "$out"
echo "$out"
