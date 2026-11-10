# Design

`keylay.pen` is the design source, opened in [Pen](https://pen.dev). Edit it through the
Pen MCP tools while the Pen app has it open.

## Editing

1. Check that the active document is `design/keylay.pen` (`get_app_state`). Pen writes to
   whichever document is active; an earlier edit landed in another open file.
2. Make the changes.
3. Save in Pen: the MCP edits stay in memory until saved. Use the menu, one action:
   `app_menu` on Pen (`dev.pencil.desktop`) with path `File > Save`.
4. Confirm the save: `design/keylay.pen`'s modification time changed and `git status`
   lists it.

## Frames and exports

Re-export every frame you changed. Export at Pen's default 2x into a scratch folder, then
resize into the repository:

| Frame | Id | Repository file | Size |
| --- | --- | --- | --- |
| 01 Brand | `sfDkS` | `design/export/01-brand.png` | 1620 px wide (`sips -Z 1620`) |
| 02 Menu Bar | `m6sYt` | `design/export/02-menu-bar.png` | 1620 px wide |
| 03 Popover | `VP93z` | `design/export/03-popover.png` | 1620 px wide |
| 04 Settings | `b2TiBg` | `design/export/04-settings.png` | 1620 px wide |
| Store 01 to 06 | `g64Yc`, `foYCz`, `tk3il`, `y98cFM`, `nRwEo`, `Wlu4A` | `docs/images/0N-*.jpg` (README) | 1440 x 900 JPEG (`sips -Z 1440 -s format jpeg -s formatOptions 85`) |

The full-size store images go to `design/export/store/` (ignored by git).
