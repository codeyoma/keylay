// KeyLay settings window: edits config.json through the app (saved automatically).

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const LAYER_COLORS = ["#8A8F98", "#3B82F6", "#10B981", "#F59E0B", "#8B5CF6", "#EC4899", "#14B8A6"];
const GITHUB_ICON =
  '<svg class="icon" viewBox="0 0 24 24"><path d="M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4"/><path d="M9 18c-4.51 2-5-2-7-2"/></svg>';
const FILE_ICON =
  '<svg class="icon" viewBox="0 0 24 24"><path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/></svg>';
const PLUS_ICON = '<svg class="icon" viewBox="0 0 24 24"><path d="M5 12h14"/><path d="M12 5v14"/></svg>';

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

// --- Devices ------------------------------------------------------------------------------

function deviceForKeyboard(k) {
  return state.snapshot?.devices.find((d) => d.keyboard === k.name) ?? null;
}

function deviceStatus(d) {
  if (!d) return "Not connected";
  const link = d.transport === "ble" ? "Bluetooth" : "USB";
  return d.layerName ? `${link} · ${d.layerName}` : `${link} · battery only`;
}

// --- Keyboards tab ------------------------------------------------------------------------

function renderList() {
  const keyboards = state.config.keyboards;
  $("kb-list").replaceChildren(
    ...keyboards.map((k, i) => {
      const d = deviceForKeyboard(k);
      const row = document.createElement("button");
      row.className = "kb";
      row.setAttribute("aria-selected", String(i === state.selected));
      row.innerHTML = `<span class="dot${d ? " on" : ""}"></span><span><span class="name"></span><span class="sub"></span></span>`;
      row.querySelector(".name").textContent = k.name || "Untitled";
      row.querySelector(".sub").textContent = deviceStatus(d);
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
      row.innerHTML = `${PLUS_ICON}<span><span class="name"></span><span class="sub"></span></span>`;
      row.querySelector(".name").textContent = d.name;
      row.querySelector(".sub").textContent = deviceStatus(d);
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

  const d = deviceForKeyboard(k);
  $("d-title").textContent = k.name || "Untitled";
  const live = $("d-live");
  live.classList.toggle("on", Boolean(d));
  live.textContent = d ? `Connected as “${d.name}”` : "Not connected";

  setValue($("f-name"), k.name);
  setValue($("f-match"), k.match);
  setPressed($("f-transport"), k.transport ?? "");
  setValue($("f-keymap"), k.keymap);
  $("f-keymap-icon").innerHTML = /github/.test(k.keymap) ? GITHUB_ICON : FILE_ICON;
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
    chip.querySelector(".swatch").style.background = LAYER_COLORS[i % LAYER_COLORS.length];
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
      row.draggable = true;
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
      // Drag a label onto another to swap them
      row.addEventListener("dragstart", (e) => {
        e.dataTransfer.setData("text/plain", String(i));
        row.classList.add("dragging");
      });
      row.addEventListener("dragend", () => row.classList.remove("dragging"));
      row.addEventListener("dragover", (e) => e.preventDefault());
      row.addEventListener("drop", (e) => {
        e.preventDefault();
        const from = Number(e.dataTransfer.getData("text/plain"));
        if (from === i) return;
        const labels = Array.from({ length: count }, (_, j) => k.batteryLabels?.[j] ?? defaults(j));
        [labels[from], labels[i]] = [labels[i], labels[from]];
        k.batteryLabels = labels;
        renderDetail();
        save();
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
  $("f-keymap-icon").innerHTML = /github/.test(k.keymap) ? GITHUB_ICON : FILE_ICON;
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

function fit(name, width) {
  if (!width) return name;
  const chars = [...name];
  // No-break spaces, like the app: they keep their width and survive the cleanup below
  return chars.length > width ? chars.slice(0, width - 1).join("") + "…" : name + "\u00a0".repeat(width - chars.length);
}

// The same rules as the tray title (src-tauri/src/tray.rs)
function previewTitle() {
  const c = state.config;
  const d = state.snapshot?.devices[0];
  const layer = d?.layerName ?? "Lower";
  const lowest = d ? Math.min(...d.batteries.map((b) => b.level).filter((l) => l != null)) : 62;
  const battery = Number.isFinite(lowest) ? `${String(lowest).padStart(3, "\u00a0")}%` : "";
  const keyboard = d?.keyboard ?? d?.name ?? "Sweep";
  return c.titleFormat
    .replace("{keyboard}", keyboard)
    .replace("{layer}", fit(layer, c.layerWidth))
    .replace("{battery}", battery)
    .split(" ")
    .filter(Boolean)
    .join(" ");
}

function renderGeneral() {
  const c = state.config;
  $("mb-title").textContent = previewTitle();
  $("mb-clock").textContent = new Date().toLocaleString([], { weekday: "short", month: "short", day: "numeric", hour: "numeric", minute: "2-digit" }).replace(",", "");
  setPressed($("g-format"), c.titleFormat);
  setValue($("g-format-custom"), c.titleFormat);
  $("g-width").textContent = c.layerWidth === 0 ? "off" : String(c.layerWidth);
  $("g-key").checked = c.keylayKey;
  setPressed($("g-interval"), String(c.keymapCheckMinutes));
  setPressed($("g-appearance"), c.appearance === "solid" ? "solid" : "glass");
}

$("g-format").addEventListener("click", (e) => {
  const b = e.target.closest("button");
  if (!b) return;
  state.config.titleFormat = b.dataset.value;
  renderGeneral();
  save();
});
$("g-format-custom").addEventListener("input", (e) => {
  state.config.titleFormat = e.target.value;
  renderGeneral();
  save();
});
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
