# KeyLay

- See the app (macOS): `scripts/dev-run.sh [popover|settings]` builds a debug bundle, opens that window and prints a screenshot path. Start the app through it, not `target/debug/keylay`: macOS kills the bare binary (TCC) once it touches Bluetooth.
- Check before committing: `npm run check` (tests, clippy, JS syntax, workflow lint); the pre-commit hook runs it.
- See the UI in a browser without the app: `npm run ui:preview` (scripts/ui-preview stubs the Tauri runtime).
- Install on this Mac: `scripts/install-macos.sh` (signed, notarized, from the disk image).
- Design (Pen, `design/keylay.pen`): `design/README.md` before editing; saving and exports are manual.
- Architecture, protocol, firmware and releases: `docs/guide.md`, `firmware/PROTOCOL.md`.
