# KeyLay

- See the app (macOS): `scripts/dev-run.sh [popover|settings]` runs the current code as "KeyLay Dev" beside the installed app (own config, copied from it), opens that window and prints a screenshot path; `stop` quits it. Start the app through it, not `target/debug/keylay`: macOS kills the bare binary (TCC) once it touches Bluetooth.
- Review rules: `CODING_STANDARDS.md`.
- Check before committing: `npm run check` (Rust and JS tests, clippy, JS syntax, workflow lint); the pre-commit hook runs it.
- See the UI in a browser without the app: `npm run ui:preview` (scripts/ui-preview stubs the Tauri runtime).
- Install on this Mac: `scripts/install-macos.sh` (signed, notarized, from the disk image).
- Design (Pen, `design/keylay.pen`): `design/README.md` before editing; saving and exports are manual.
- Architecture, protocol, firmware and releases: `docs/guide.md`, `firmware/PROTOCOL.md`.
