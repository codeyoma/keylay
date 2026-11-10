# KeyLay guide

A menu bar (macOS) and tray (Windows) app for QMK and ZMK keyboards. It shows:

- the active layer, live as you type,
- the battery levels of wireless keyboards, per half on a split,
- a keymap cheat sheet: the keys of the current layer, or of every layer, drawn by
  [keymap-drawer](https://github.com/caksoylar/keymap-drawer).

On macOS the menu bar shows a keyboard icon and the layer and lowest battery level as text,
e.g. `Lower   62%`. The text is monospaced and the layer name is cut or padded to 6
characters (`layerWidth`), so the menu bar item keeps its width when the layer changes.
Windows trays cannot show text, so the icon becomes a badge: the layer's
first letter on a color per layer, with a battery bar; the tooltip has the details.

Click the icon to open the cheat sheet:

- a keyboard switcher, the link (USB, Bluetooth or not connected), pin and close;
- the layer name with its layer color, the other active layers, and a gauge per battery;
- a row of all layers: click one to preview its keys, click again to follow the keyboard;
- the keymap: the keyboard's keymap-drawer SVG with its geometry kept (column stagger,
  angled thumb keys, combos), restyled; the key that holds a layer takes that layer's color;
- "Current" shows only the active layer and sizes the window to it; "All" shows every layer
  and scrolls to the active one. A keyboard that is not connected has no current layer, so it
  shows all layers with none highlighted.

Pin it to keep it on top while you type in another app. Drag the cheat sheet anywhere and it
keeps opening there (menu > Move Cheat Sheet Back to the Menu Bar undoes that); it also keeps
the width you drag it to (in "All", also the height). Both are kept in `window.json` next to
`config.json`. Right-click the icon for the menu
(reload, config, launch at login, quit).

The design is in `../design/keylay.pen` (pen.dev): name, app icon, menu bar glyph, Windows
badge and the popover. `../design/assets/` holds the exported icon and glyph.

## How it gets the data

Firmware does not tell the computer which layer is active, so each keyboard needs a small
hook (see [firmware/PROTOCOL.md](../firmware/PROTOCOL.md)):

| Keyboard | Link | Hook | Layer | Battery |
|---|---|---|---|---|
| QMK | USB | raw HID (`firmware/qmk/keylay.c`) | yes | - |
| ZMK | Bluetooth | GATT service (`firmware/zmk`) | yes | every half, and the dongle |
| ZMK without the hook | Bluetooth | none; standard Battery Service | no | yes |

The keyboards stay connected to the OS as usual; the app only reads from them.

The cheat sheet is the keymap-drawer SVG of each keyboard, loaded from a URL or a file. Its
layer order is the firmware's, so it also names the layers.

The app checks the URLs every 15 minutes and whenever the cheat sheet opens (at most once a
minute), with the ETag of the copy it has: GitHub answers 304 with no body when nothing
changed. A pushed SVG shows up within about 5-20 minutes (GitHub caches raw files for 5
minutes); Reload checks right away. The last copy and its ETag are kept in the cache folder,
so the app starts with it and works offline.

## Firmware

In use on:

- Charybdis 3x5 (QMK): `qmk/keyboards/bastardkb/charybdis/3x5/keymaps/default/keymap.c`
  and `rules.mk` (`RAW_ENABLE = yes`).
- Charybdis, Sweep and moNa2 (ZMK): each `config/west.yml` pulls this repository as a
  module. It runs on the Charybdis and Sweep dongles (which report their own battery and
  both halves') and on the moNa2's right half (the central).

The KeyLay key opens or closes the cheat sheet: `KL_OPEN` in QMK, `&keylay` in ZMK. On these
keyboards it is on the Adj layer, at the position of `/`.

For another keyboard:

- **QMK**: paste `firmware/qmk/keylay.c` into `keymap.c` and add `RAW_ENABLE = yes` to the
  keymap's `rules.mk`. With VIA enabled, rename `raw_hid_receive` to `raw_hid_receive_kb`.
- **ZMK** (v0.3): this repository is a ZMK module (`zephyr/module.yml`, code in
  `firmware/zmk`). Add it to the config's `config/west.yml`:

  ```yaml
  remotes:
    - name: codeyoma
      url-base: https://github.com/codeyoma
  projects:
    - name: keylay
      remote: codeyoma
      revision: main
  ```

  `CONFIG_ZMK_KEYLAY` turns itself on for the central. For the KeyLay key, declare the
  behavior in the keymap (`keylay: keylay { compatible = "zmk,behavior-keylay";
  #binding-cells = <0>; };` inside `behaviors`) and bind `&keylay`. For peripheral
  batteries, set `CONFIG_ZMK_SPLIT_BLE_CENTRAL_BATTERY_LEVEL_FETCHING=y` on the central.

After flashing a ZMK keyboard, the computer may still have the old list of Bluetooth
services. If the layer does not show up, turn Bluetooth off and on, or remove and pair the
keyboard again.

## Config

Use the settings window (menu > Settings…, or the sliders button in the cheat sheet). It
edits the same `config.json`, saved automatically:

- Keyboards: connected keyboards that are not set up appear under "Found, not set up"; a
  keymap URL is checked as you paste it and its layers are listed; click a layer to rename
  it; drag battery names to swap them.
- General: menu bar title and layer width (with a live preview), launch at login, the KeyLay
  key, and how often keymaps are checked (`keymapCheckMinutes`, 0 = never).

The file itself is `config.json` in the app's config folder (menu > Open Config File):

- macOS: `~/Library/Application Support/dev.yoma.keylay/config.json`
- Windows: `%APPDATA%\dev.yoma.keylay\config.json`

```json
{
  "titleFormat": "{layer} {battery}",
  "layerWidth": 6,
  "keyboards": [
    {
      "name": "Sweep",
      "match": "Sweep",
      "transport": "ble",
      "keymap": "https://raw.githubusercontent.com/codeyoma/keyboard-zmk-sweep/main/sweep_keymap.svg",
      "batteryLabels": ["Dongle", "Left", "Right"],
      "layerNames": []
    }
  ]
}
```

- `match`: part of the USB product name or Bluetooth name, any case.
- `transport`: `usb` or `ble`, for keyboards with the same name. The QMK Charybdis
  ("Charybdis Nano (QMK)", USB) and the ZMK one ("Charybdis Nano", Bluetooth) are told apart
  this way. Empty matches both.
- `keymap`: keymap-drawer SVG, a URL or a file path (`~` works).
- `batteryLabels`: names of the batteries in the order the firmware sends them: the
  keyboard or central first, then the peripherals in pairing order. Swap them if the halves
  come out reversed.
- `layerNames`: only needed when there is no SVG, or to rename layers.
- `titleFormat` (macOS menu bar): `{keyboard}`, `{layer}`, `{battery}` (the lowest level,
  always 4 characters).
- `layerWidth`: characters for `{layer}`; longer names end in `…` (`Colemak` becomes
  `Colem…`). 0 shows names as they are.

With several keyboards connected, the tray shows the one whose layer changed last.

## Build

Needs Rust (`rustup`) and Node.js.

```bash
npm install
npm run build      # src-tauri/target/release/bundle/: KeyLay.app / .dmg, or the Windows installer
npm run dev        # run from source
cd src-tauri && cargo test
```

The GitHub Actions workflow builds the macOS `.dmg` (universal) and the Windows installer.

On first launch macOS asks for Bluetooth access; it is needed for the ZMK keyboards.

Set `KEYLAY_DEBUG=1` to print every state change to stderr, and `KEYLAY_SHOW_POPOVER=1` to
open the popover (pinned) right after launch:

```bash
open -n KeyLay.app --env KEYLAY_DEBUG=1 --stderr /tmp/keylay.log
```

## Notes

- Bluetooth keyboards are found while they are connected to the OS; a sleeping keyboard
  disappears and comes back when it wakes.
- QMK sends a report on every layer change. macOS and Windows read raw HID reports all the
  time, so this costs nothing there; on Linux, a host that never reads them would make each
  report wait up to 100 ms in the firmware.
