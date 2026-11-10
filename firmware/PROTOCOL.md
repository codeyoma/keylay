# KeyLay protocol

The keyboard reports its state; the app only listens (and, over USB, asks once).

## State message

The same bytes go over USB raw HID and over Bluetooth.

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | Magic `0x4B` (`K`) |
| 1 | 1 | Protocol version, `0x01` |
| 2 | 1 | Message type, `0x01` = state |
| 3 | 1 | Highest active layer index |
| 4 | 4 | Active layer bitmask, little endian |
| 8 | 1 | Battery count N |
| 9 | N | Battery levels, 0-100; `0xFF` = unknown |

Battery 0 is the keyboard itself, or the split central (a dongle or the central half). Batteries
1 and up are the split peripherals, in the order the central numbers them (the order they
were first paired).

The layer index is the keymap order, which is also the order of the layers in a
keymap-drawer SVG; the app uses that to name the layer.

Later versions may append fields. The app ignores bytes it does not know.

## USB raw HID (QMK)

- Usage page `0xFF60`, usage `0x61` (QMK's default raw HID interface), 32-byte reports.
- The keyboard sends a state report whenever the layer state changes.
- The app sends `4B 01 02` (magic, version, "get state") padded to 32 bytes when it opens
  the device; the keyboard answers with a state report.

## Bluetooth GATT (ZMK)

- Service `9f3c0001-5b2e-4a6d-8f1c-3e7a2b9d4c60`
- Characteristic `9f3c0002-5b2e-4a6d-8f1c-3e7a2b9d4c60`: read and notify, encrypted link
  required. It holds the state message. A notification is sent whenever the layer state or
  a battery level changes, and right after the host subscribes.

Keyboards without the service still show their battery levels when they have the standard
Battery Service (`0x180F`), as long as they are in the app's config.
