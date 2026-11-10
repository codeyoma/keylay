#!/bin/bash
# Every check the repository has, in one command (`npm run check`). The pre-commit hook and
# CI run it too.
set -euo pipefail
cd "$(dirname "$0")/.."

# cargo lives in the rustup toolchain, which is not always on PATH
if ! command -v cargo >/dev/null; then
  PATH="$(dirname "$(rustup which cargo)"):$PATH"
  export PATH
fi

step() { printf '\n== %s\n' "$1"; }

step "Rust tests"
(cd src-tauri && cargo test --quiet)

step "Clippy"
(cd src-tauri && cargo clippy --quiet --all-targets -- -D warnings)

step "JavaScript syntax"
for f in ui/*.js scripts/ui-preview/*.mjs scripts/ui-preview/*.js; do
  node --check "$f"
done

step "Rules"
failed=0
# Reordering in the windows goes through draggable() in ui/common.js: HTML drag and drop
# is not reliable in the webview
if grep -nE 'draggable *= *true|"dragstart"|"dragover"|"drop"' ui/*.js ui/*.html; then
  echo "Use draggable() from ui/common.js for drag and drop (pointer events)." >&2
  failed=1
fi
# Third-party actions are pinned to a commit SHA, so a moved tag cannot change a build
if grep -nE '^\s*-?\s*uses: ' .github/workflows/*.yml | grep -vE 'uses: \./|@[0-9a-f]{40}( |$)'; then
  echo "Pin each action to a full commit SHA: uses: owner/action@<sha> # vN" >&2
  failed=1
fi
[ "$failed" = 0 ] || exit 1

step "Workflows"
if command -v actionlint >/dev/null; then
  actionlint
else
  echo "actionlint is not installed (brew install actionlint); skipped" >&2
  [ -z "${CI:-}" ] || exit 1
fi

printf '\nAll checks passed.\n'
