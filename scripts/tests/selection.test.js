// The cheat sheet's selection rules (ui/selection.js). Run by `npm run check`.
const test = require("node:test");
const assert = require("node:assert/strict");
const { select, shownLayer, previewAfterClick } = require("../../ui/selection.js");

const keyboard = (name) => ({ name, layers: [] });
const device = (key, layer, configured = true) => ({ key, name: key, keyboard: configured ? key : null, layer });
const snapshot = (keyboards, devices) => ({ keyboards: keyboards.map(keyboard), devices });
const fresh = { selected: null, userPicked: false, preview: null, liveLayer: undefined };

test("follows the keyboard whose layer changed last", () => {
  const s = snapshot(["Sweep", "Mona"], [device("Mona", 2), device("Sweep", 0)]);
  const view = select(s, fresh);
  assert.equal(view.picks.selected, "Mona");
  assert.equal(view.live, 2);
});

test("a pick in the switcher wins until the picked keyboard is gone", () => {
  const s = snapshot(["Sweep", "Mona"], [device("Mona", 2)]);
  const picked = { ...fresh, selected: "Sweep", userPicked: true };
  assert.equal(select(s, picked).picks.selected, "Sweep");
  const gone = snapshot(["Mona"], [device("Mona", 2)]);
  assert.equal(select(gone, picked).picks.selected, "Mona");
});

test("without connected keyboards it shows the first configured one, with no live layer", () => {
  const view = select(snapshot(["Sweep", "Mona"], []), fresh);
  assert.equal(view.picks.selected, "Sweep");
  assert.equal(view.live, null);
  assert.equal(view.device, null);
});

test("a connected keyboard missing from the config gets its own entry", () => {
  const view = select(snapshot(["Sweep"], [device("Corne", 1, false)]), fresh);
  assert.deepEqual(view.entries.map((e) => e.key), ["Sweep", "Corne"]);
  assert.equal(view.picks.selected, "Corne");
  assert.equal(view.keyboard, null);
});

test("a layer preview ends when the keyboard changes layer", () => {
  const s1 = snapshot(["Sweep"], [device("Sweep", 0)]);
  const first = select(s1, fresh).picks;
  const previewing = { ...first, preview: 3 };
  assert.equal(select(s1, previewing).picks.preview, 3);
  const s2 = snapshot(["Sweep"], [device("Sweep", 1)]);
  assert.equal(select(s2, previewing).picks.preview, null);
});

test("the layer shown: preview, else live, else the first in Current", () => {
  assert.equal(shownLayer({ preview: 4 }, 1, "current"), 4);
  assert.equal(shownLayer({ preview: null }, 1, "current"), 1);
  assert.equal(shownLayer({ preview: null }, null, "current"), 0);
  assert.equal(shownLayer({ preview: null }, null, "all"), null);
});

test("clicking a chip previews it; again, or the live layer, ends the preview", () => {
  assert.equal(previewAfterClick({ preview: null }, 3, 0), 3);
  assert.equal(previewAfterClick({ preview: 3 }, 3, 0), null);
  assert.equal(previewAfterClick({ preview: 3 }, 0, 0), null);
  assert.equal(previewAfterClick({ preview: 3 }, 5, 0), 5);
});
