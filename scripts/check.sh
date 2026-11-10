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

step "Workflows"
if command -v actionlint >/dev/null; then
  actionlint
else
  echo "actionlint is not installed (brew install actionlint); skipped" >&2
  [ -z "${CI:-}" ] || exit 1
fi

printf '\nAll checks passed.\n'
