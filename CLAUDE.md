# KeyLay

- See the app (macOS): `scripts/dev-run.sh [popover|settings]` builds a debug bundle, opens that window and prints a screenshot path. Start the app through it, not `target/debug/keylay`: macOS kills the bare binary (TCC) once it touches Bluetooth.
- Install on this Mac: `scripts/install-macos.sh` (signed, notarized, from the disk image).
- Architecture, protocol, firmware and releases: `docs/guide.md`, `firmware/PROTOCOL.md`.
