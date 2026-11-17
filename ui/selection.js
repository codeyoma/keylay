// What the cheat sheet shows: which keyboard, and which layer. Pure functions of the snapshot
// and the user's picks, so the rules are tested in node (scripts/tests/selection.test.js);
// app.js only draws the result.
//
// picks: {
//   selected,   // switcher key: config name, or device name for a keyboard not in the config
//   userPicked, // a pick in the switcher (or the KeyLay key) wins over "follow the active
//               // keyboard" until the cheat sheet is opened again
//   preview,    // layer index clicked in the layer row; null follows the keyboard
//   liveLayer,  // the keyboard's layer when last drawn, to drop the preview when it changes
// }

// The connected device for a keyboard: `key` is its config name, or the device name for a
// keyboard that is not in the config
const deviceFor = (snapshot, key) => snapshot?.devices.find((d) => d.key === key) ?? null;

// Switcher entries: the keyboards in the config, then connected keyboards that are not in it.
function entriesOf(snapshot) {
  const entries = snapshot.keyboards.map((k) => ({ key: k.name, keyboard: k }));
  for (const d of snapshot.devices) {
    if (!d.keyboard && !entries.some((e) => e.key === d.name)) entries.push({ key: d.name, keyboard: null });
  }
  return entries;
}

// The keyboard to show and its live layer, with the picks updated for it: without a pick (or
// when the picked keyboard is gone) it follows the keyboard whose layer changed last, and a
// layer preview ends when the keyboard changes layer.
function select(snapshot, picks) {
  const entries = entriesOf(snapshot);
  let { selected, userPicked, preview, liveLayer } = picks;
  if (!userPicked || !entries.some((e) => e.key === selected)) {
    const active = snapshot.devices[0];
    selected = active ? active.key : (entries[0]?.key ?? null);
  }
  const entry = entries.find((e) => e.key === selected) ?? null;
  const device = deviceFor(snapshot, selected);
  const live = device?.layer ?? null;
  if (live !== liveLayer) {
    liveLayer = live;
    preview = null;
  }
  return {
    entries,
    entry,
    keyboard: entry?.keyboard ?? null,
    device,
    live,
    picks: { selected, userPicked, preview, liveLayer },
  };
}

// The layer "Current" shows: the one clicked in the layer row, else the keyboard's, else (not
// connected, or no layer reports) the first. "All" scrolls to it, or stays at the top.
function shownLayer(picks, live, mode) {
  return picks.preview ?? live ?? (mode === "current" ? 0 : null);
}

// A click on a layer chip: previews that layer, or ends the preview when it is clicked again
// or is the live layer.
function previewAfterClick(picks, index, live) {
  return picks.preview === index || index === live ? null : index;
}

if (typeof module === "object") module.exports = { deviceFor, entriesOf, select, shownLayer, previewAfterClick };
