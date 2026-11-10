// KeyLay settings window: edits config.json through the app (saved automatically).

const state = {
  config: null,
  snapshot: null,
  selected: 0, // index into config.keyboards
  probe: {}, // keymap source -> {layers} | {error} | {pending: true}
  tab: "keyboards",
};

// --- Saving -------------------------------------------------------------------------------

let saveTimer = null;
function save() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(async () => {
    try {
      await invoke("save_config", { config: state.config });
      const saved = $("saved");
      saved.textContent = "Saved";
      saved.classList.add("flash");
      setTimeout(() => {
        saved.textContent = "Saved automatically";
        saved.classList.remove("flash");
      }, 1200);
    } catch (e) {
      $("saved").textContent = `Not saved: ${e}`;
    }
  }, 400);
}

// --- Keyboards tab ------------------------------------------------------------------------

function renderList() {
  const keyboards = state.config.keyboards;
  $("kb-list").replaceChildren(
    ...keyboards.map((k, i) => {
      const d = deviceFor(state.snapshot, k.name);
      const row = document.createElement("button");
      row.className = "kb";
      row.setAttribute("aria-selected", String(i === state.selected));
      row.innerHTML = `<span class="dot${d ? " on" : ""}"></span><span><span class="name"></span><span class="sub"></span></span>`;
      row.querySelector(".name").textContent = k.name || "Untitled";
      row.querySelector(".sub").textContent = (d?.status ?? "Not connected");
      row.addEventListener("click", () => {
        state.selected = i;
        renderKeyboards();
      });
      return row;
    }),
  );

  const found = (state.snapshot?.devices ?? []).filter((d) => !d.keyboard);
  $("found-section").hidden = found.length === 0;
  $("found-list").replaceChildren(
    ...found.map((d) => {
      const row = document.createElement("button");
      row.className = "kb found";
      row.innerHTML = `${icon("plus")}<span><span class="name"></span><span class="sub"></span></span>`;
      row.querySelector(".name").textContent = d.name;
      row.querySelector(".sub").textContent = (d?.status ?? "Not connected");
      row.addEventListener("click", () =>
        addKeyboard({ name: d.name, match: d.name, transport: d.transport }),
      );
      return row;
    }),
  );
}

function addKeyboard(partial) {
  state.config.keyboards.push({
    name: "New keyboard",
    match: "",
    transport: "",
    keymap: "",
    batteryLabels: [],
    layerNames: [],
    ...partial,
  });
  state.selected = state.config.keyboards.length - 1;
  renderKeyboards();
  save();
  $("f-name").focus();
}

function renderDetail() {
  const k = state.config.keyboards[state.selected];
  $("detail").hidden = !k;
  $("empty").hidden = Boolean(k);
  if (!k) return;

  const d = deviceFor(state.snapshot, k.name);
  $("d-title").textContent = k.name || "Untitled";
  const live = $("d-live");
  live.classList.toggle("on", Boolean(d));
  live.textContent = d ? `Connected as “${d.name}”` : "Not connected";

  setValue($("f-name"), k.name);
  setValue($("f-match"), k.match);
  setPressed($("f-transport"), k.transport ?? "");
  setValue($("f-keymap"), k.keymap);
  $("f-keymap-icon").innerHTML = sourceIcon(k.keymap);
  renderKeymapStatus(k);
  renderLayers(k);
  renderBatteries(k, d);
}

// Only touch an input when its value really changed, so typing is not interrupted
function setValue(input, value) {
  if (document.activeElement !== input && input.value !== (value ?? "")) input.value = value ?? "";
}

function setPressed(group, value) {
  for (const b of group.querySelectorAll("button")) b.setAttribute("aria-pressed", String(b.dataset.value === String(value)));
}

// The layer names a keymap source has, from the app's last check or a probe of our own
function keymapLayers(k) {
  const loaded = state.snapshot?.keyboards.find((x) => x.name === k.name);
  const probe = state.probe[k.keymap];
  if (probe?.layers) return probe.layers;
  if (loaded?.layers?.length && !k.layerNames?.length) return loaded.layers;
  return probe?.layers ?? [];
}

function renderKeymapStatus(k) {
  const status = $("f-keymap-status");
  const p = state.probe[k.keymap];
  status.className = "status";
  if (!k.keymap) {
    status.textContent = "No keymap: the cheat sheet stays empty";
  } else if (!p || p.pending) {
    status.textContent = "Checking…";
  } else if (p.error) {
    status.classList.add("bad");
    status.textContent = p.error;
  } else {
    status.classList.add("ok");
    status.textContent = `Loaded · ${p.layers.length} layers · checked ${ago(p.at)}`;
  }
}

function ago(ms) {
  const s = (Date.now() - ms) / 1000;
  return s < 45 ? "just now" : `${Math.round(s / 60)} min ago`;
}

async function probe(source) {
  if (!source || state.probe[source]?.pending) return;
  state.probe[source] = { pending: true };
  try {
    state.probe[source] = { layers: await invoke("check_keymap", { source }), at: Date.now() };
  } catch (e) {
    state.probe[source] = { error: String(e), at: Date.now() };
  }
  renderDetail();
}

function renderLayers(k) {
  const fromSvg = keymapLayers(k);
  const names = k.layerNames?.length ? k.layerNames : fromSvg;
  const box = $("f-layers");
  const chips = names.map((n, i) => {
    const chip = document.createElement("button");
    chip.className = "chip" + (k.layerNames?.length && n !== fromSvg[i] ? " renamed" : "");
    chip.innerHTML = `<span class="swatch"></span><span class="n"></span>`;
    chip.querySelector(".swatch").style.background = layerColor(state.snapshot, i);
    chip.querySelector(".n").textContent = n;
    chip.title = `Layer ${i}`;
    chip.addEventListener("click", () => renameLayer(chip, k, names, i));
    return chip;
  });
  if (k.layerNames?.length) {
    const reset = document.createElement("button");
    reset.className = "reset";
    reset.textContent = "Use the keymap's names";
    reset.addEventListener("click", () => {
      k.layerNames = [];
      renderDetail();
      save();
    });
    chips.push(reset);
  }
  if (names.length === 0) {
    const none = document.createElement("span");
    none.className = "hint";
    none.textContent = "No layers yet: add a keymap above.";
    chips.push(none);
  }
  box.replaceChildren(...chips);
  // Every name gets the width of the longest one plus room to type, so a chip keeps its size
  // when it turns into a field
  // (measured off-screen, so it also works while the tab is hidden)
  const widths = names.map((n) => textWidth(n, "12px " + getComputedStyle(document.body).fontFamily));
  box.style.setProperty("--name-w", `${Math.ceil(Math.max(0, ...widths)) + 16}px`);
}

const measureCanvas = document.createElement("canvas").getContext("2d");
function textWidth(text, font) {
  measureCanvas.font = font;
  return measureCanvas.measureText(text).width;
}

function renameLayer(chip, k, names, i) {
  const input = document.createElement("input");
  input.value = names[i];
  chip.querySelector(".n").replaceWith(input);
  input.focus();
  input.select();
  const done = (commit) => {
    if (commit && input.value.trim() && input.value.trim() !== names[i]) {
      k.layerNames = [...names];
      k.layerNames[i] = input.value.trim();
      save();
    }
    renderDetail();
  };
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") done(true);
    if (e.key === "Escape") done(false);
  });
  input.addEventListener("blur", () => done(true));
}

function renderBatteries(k, d) {
  const levels = d?.batteries ?? [];
  const count = Math.max(levels.length, k.batteryLabels?.length ?? 0);
  const defaults = (i) => (count === 1 ? "Battery" : i === 0 ? "Central" : `Peripheral ${i}`);
  const box = $("f-batteries");
  if (count === 0) {
    const none = document.createElement("span");
    none.className = "hint";
    none.textContent = "None reported. A wired keyboard has no batteries.";
    box.replaceChildren(none);
    return;
  }
  box.replaceChildren(
    ...Array.from({ length: count }, (_, i) => {
      const row = document.createElement("div");
      row.className = "battery";
      row.innerHTML =
        '<svg class="icon grip" viewBox="0 0 24 24"><circle cx="9" cy="6" r="1"/><circle cx="9" cy="12" r="1"/><circle cx="9" cy="18" r="1"/><circle cx="15" cy="6" r="1"/><circle cx="15" cy="12" r="1"/><circle cx="15" cy="18" r="1"/></svg><input spellcheck="false" /><span class="now"></span>';
      const input = row.querySelector("input");
      input.value = k.batteryLabels?.[i] ?? defaults(i);
      input.addEventListener("input", () => {
        k.batteryLabels = Array.from({ length: count }, (_, j) => k.batteryLabels?.[j] ?? defaults(j));
        k.batteryLabels[i] = input.value;
        save();
      });
      const level = levels[i]?.level;
      row.querySelector(".now").textContent = level == null ? "" : `${level}%`;
      row.dataset.index = String(i);
      // Drag a label to another place, e.g. when the halves are reversed
      draggable(row, {
        zones: () => [box],
        handle: ".grip",
        onDrop: () => {
          const labels = Array.from({ length: count }, (_, j) => k.batteryLabels?.[j] ?? defaults(j));
          k.batteryLabels = [...box.querySelectorAll(".battery")].map((r) => labels[Number(r.dataset.index)]);
          renderDetail();
          save();
        },
      });
      return row;
    }),
  );
}

function renderKeyboards() {
  renderList();
  renderDetail();
  const k = state.config.keyboards[state.selected];
  if (k?.keymap && !state.probe[k.keymap]) probe(k.keymap);
}

function bindField(id, key, after) {
  $(id).addEventListener("input", (e) => {
    const k = state.config.keyboards[state.selected];
    if (!k) return;
    k[key] = e.target.value;
    after?.(k);
    renderList();
    $("d-title").textContent = k.name || "Untitled";
    save();
  });
}

let probeTimer = null;
bindField("f-name", "name");
bindField("f-match", "match");
bindField("f-keymap", "keymap", (k) => {
  $("f-keymap-icon").innerHTML = sourceIcon(k.keymap);
  clearTimeout(probeTimer);
  probeTimer = setTimeout(() => {
    probe(k.keymap.trim());
    renderKeymapStatus(k);
  }, 600);
});

$("f-transport").addEventListener("click", (e) => {
  const b = e.target.closest("button");
  const k = state.config.keyboards[state.selected];
  if (!b || !k) return;
  k.transport = b.dataset.value;
  setPressed($("f-transport"), k.transport);
  save();
});

$("add").addEventListener("click", () => addKeyboard({}));

$("remove").addEventListener("click", () => {
  const k = state.config.keyboards[state.selected];
  if (!k || !confirm(`Remove “${k.name}” from KeyLay? The keyboard itself is not changed.`)) return;
  state.config.keyboards.splice(state.selected, 1);
  state.selected = Math.max(0, state.selected - 1);
  renderKeyboards();
  save();
});

// --- General tab --------------------------------------------------------------------------

// The title comes from the app, so the preview follows the same rules as the menu bar
let titleRequest = 0;
async function renderTitlePreview() {
  const c = state.config;
  const request = ++titleRequest;
  const title = await invoke("preview_title", { format: c.titleFormat, layerWidth: c.layerWidth });
  if (request === titleRequest) $("mb-title").textContent = title;
}

function renderGeneral() {
  const c = state.config;
  renderTitlePreview();
  $("mb-clock").textContent = new Date().toLocaleString([], { weekday: "short", month: "short", day: "numeric", hour: "numeric", minute: "2-digit" }).replace(",", "");
  renderTitleParts();
  $("g-width").textContent = c.layerWidth === 0 ? "off" : String(c.layerWidth);
  $("g-key").checked = c.keylayKey;
  setPressed($("g-interval"), String(c.keymapCheckMinutes));
  setPressed($("g-appearance"), c.appearance === "solid" ? "solid" : "glass");
}

// --- Title parts: the title format as parts to drag into order ------------------------------

const TITLE_PARTS = [
  { token: "{layer}", label: "Layer" },
  { token: "{keyboard}", label: "Keyboard" },
  { token: "{battery}", label: "Battery" },
];
const GRIP =
  '<svg class="icon grip" viewBox="0 0 24 24"><circle cx="9" cy="6" r="1"/><circle cx="9" cy="12" r="1"/><circle cx="9" cy="18" r="1"/><circle cx="15" cy="6" r="1"/><circle cx="15" cy="12" r="1"/><circle cx="15" cy="18" r="1"/></svg>';

// The parts in the format, in order ("{layer} {battery}" -> ["{layer}", "{battery}"])
function shownTokens(format) {
  return (format.match(/\{(layer|keyboard|battery)\}/g) ?? []).filter((t, i, all) => all.indexOf(t) === i);
}

function setTitleTokens(tokens) {
  state.config.titleFormat = tokens.join(" ");
  renderGeneral();
  save();
}

function partElement(part) {
  const el = document.createElement("div");
  el.className = "part";
  el.dataset.token = part.token;
  el.innerHTML = `${GRIP}<span></span>`;
  el.querySelector("span").textContent = part.label;
  draggable(el, {
    zones: () => [$("g-parts"), $("g-spare")],
    onDrop: () => {
      const next = [...$("g-parts").querySelectorAll(".part")].map((p) => p.dataset.token);
      // The title needs at least one part
      if (next.length > 0) setTitleTokens(next);
      else renderTitleParts();
    },
    onClick: () => {
      // Move it to the other row
      const shown = shownTokens(state.config.titleFormat);
      const token = part.token;
      const next = shown.includes(token) ? shown.filter((t) => t !== token) : [...shown, token];
      if (next.length > 0) setTitleTokens(next);
    },
  });
  return el;
}

function renderTitleParts() {
  if (document.querySelector(".part.drag-lifted")) return; // a drag owns the parts until it ends
  const shown = shownTokens(state.config.titleFormat);
  const byToken = (t) => TITLE_PARTS.find((p) => p.token === t);
  $("g-parts").replaceChildren(...shown.map((t) => partElement(byToken(t))));
  $("g-spare").replaceChildren(...TITLE_PARTS.filter((p) => !shown.includes(p.token)).map(partElement));
}

for (const [id, step] of [["g-width-dec", -1], ["g-width-inc", 1]]) {
  $(id).addEventListener("click", () => {
    state.config.layerWidth = Math.min(12, Math.max(0, state.config.layerWidth + step));
    renderGeneral();
    save();
  });
}
$("g-key").addEventListener("change", (e) => {
  state.config.keylayKey = e.target.checked;
  save();
});
$("g-interval").addEventListener("click", (e) => {
  const b = e.target.closest("button");
  if (!b) return;
  state.config.keymapCheckMinutes = Number(b.dataset.value);
  renderGeneral();
  save();
});
$("g-login").addEventListener("change", async (e) => {
  try {
    await invoke("set_login", { enabled: e.target.checked });
  } catch {
    e.target.checked = !e.target.checked;
  }
});
$("g-appearance").addEventListener("click", (e) => {
  const b = e.target.closest("button");
  if (!b) return;
  state.config.appearance = b.dataset.value;
  renderGeneral();
  save();
});
$("g-json").addEventListener("click", () => invoke("open_config"));

// --- Tabs and startup ---------------------------------------------------------------------

function showTab(tab) {
  state.tab = tab;
  for (const t of document.querySelectorAll(".tab")) t.setAttribute("aria-selected", String(t.dataset.tab === tab));
  $("tab-keyboards").hidden = tab !== "keyboards";
  $("tab-general").hidden = tab !== "general";
  if (tab === "general") renderGeneral();
}
for (const t of document.querySelectorAll(".tab")) t.addEventListener("click", () => showTab(t.dataset.tab));

listen("snapshot", (e) => {
  state.snapshot = e.payload;
  document.documentElement.dataset.appearance = e.payload.appearance;
  if (!state.config) return;
  renderList();
  renderDetail();
  if (state.tab === "general") renderGeneral();
});

Promise.all([invoke("get_config"), invoke("get_snapshot"), invoke("get_login")]).then(([config, snapshot, login]) => {
  state.config = config;
  state.snapshot = snapshot;
  document.documentElement.dataset.appearance = snapshot.appearance;
  $("g-login").checked = login;
  showTab("keyboards");
  renderKeyboards();
});
