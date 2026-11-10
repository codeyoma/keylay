// KeyLay popover: keyboard switcher, active layer, batteries, layer row, and the
// keymap-drawer cheat sheet (its geometry kept as drawn, restyled to the design).

const state = {
  snapshot: null,
  selected: null, // switcher key: config name, or device name for keyboards not in the config
  userPicked: false, // a pick in the switcher wins over "follow the active keyboard" until reopened
  mode: read("mode", "current"),
  preview: null, // layer index clicked in the layer row; null follows the keyboard
  liveLayer: undefined, // to drop the preview when the keyboard changes layer
  sheetFor: undefined, // "name#revision" of the SVG in #sheet
  svg: null,
  fullViewBox: null,
  groups: [], // <g class="layer-..."> per layer, in keymap order
  boxes: [], // their key area (label left out) in SVG coordinates
  labels: [], // and with the label, for "All"
  shown: null,
};

function read(key, fallback) {
  try {
    return localStorage.getItem(key) ?? fallback;
  } catch {
    return fallback;
  }
}

function write(key, value) {
  try {
    localStorage.setItem(key, value);
  } catch {}
}

// Switcher entries: the keyboards in the config, then connected keyboards that are not in it.
function entriesOf(snapshot) {
  const entries = snapshot.keyboards.map((k) => ({ key: k.name, keyboard: k }));
  for (const d of snapshot.devices) {
    if (!d.keyboard && !entries.some((e) => e.key === d.name)) entries.push({ key: d.name, keyboard: null });
  }
  return entries;
}

async function render() {
  const snapshot = state.snapshot;
  if (!snapshot) return;
  document.documentElement.dataset.appearance = snapshot.appearance;

  const entries = entriesOf(snapshot);
  if (!state.userPicked || !entries.some((e) => e.key === state.selected)) {
    const active = snapshot.devices[0];
    state.selected = active ? active.key : (entries[0]?.key ?? null);
  }
  const entry = entries.find((e) => e.key === state.selected) ?? null;
  const keyboard = entry?.keyboard ?? null;
  const device = deviceFor(snapshot, state.selected);

  const live = device?.layer ?? null;
  if (live !== state.liveLayer) {
    state.liveLayer = live;
    state.preview = null;
  }
  document.documentElement.style.setProperty("--layer", layerColor(snapshot, live ?? 0));

  renderTopBar(snapshot, entries, entry, device);
  renderHero(device, keyboard, entry);
  renderChips(device, keyboard);
  renderFooter(snapshot, keyboard);
  $("pin").setAttribute("aria-pressed", String(snapshot.pinned));
  await renderSheet(keyboard, device);
}

function linkHtml(device) {
  if (!device) return `${icon("bleOff")}<span>Not connected</span>`;
  return device.transport === "ble" ? `${icon("ble")}<span>Bluetooth</span>` : `${icon("usb")}<span>USB</span>`;
}

function renderTopBar(snapshot, entries, entry, device) {
  $("switcher-name").textContent = entry?.key ?? "No keyboard";
  $("switcher-dot").classList.toggle("on", Boolean(device));
  $("link").innerHTML = linkHtml(device);

  const menu = $("menu");
  menu.replaceChildren(
    ...entries.map((e) => {
      const d = deviceFor(snapshot, e.key);
      const item = document.createElement("button");
      item.className = "menu-item";
      item.setAttribute("role", "option");
      item.setAttribute("aria-selected", String(e.key === state.selected));
      item.innerHTML = `<span class="dot${d ? " on" : ""}"></span><span class="name"></span><span class="meta"></span>`;
      item.querySelector(".name").textContent = e.key;
      item.querySelector(".meta").textContent = d ? (d.layerName ?? (d.transport === "ble" ? "Bluetooth" : "USB")) : "";
      item.addEventListener("click", () => {
        state.selected = e.key;
        state.userPicked = true;
        state.preview = null;
        closeMenu();
        render();
      });
      return item;
    }),
  );
}

function renderHero(device, keyboard, entry) {
  const bar = $("layer-bar");
  const name = $("layer-name");
  const detail = $("layer-detail");
  const layers = keyboard?.layers ?? [];

  if (device?.layer != null) {
    bar.classList.remove("off");
    name.classList.remove("off");
    name.textContent = device.layerName;
    // Other active layers, besides the base layer
    const others = [];
    for (let i = 1; i < 32; i++) {
      if (i !== device.layer && device.layerMask & (1 << i)) others.push(layers[i] ?? `Layer ${i}`);
    }
    detail.textContent =
      device.layer === 0 ? "Layer 0 · base" : `Layer ${device.layer}` + (others.length ? ` · with ${others.join(", ")}` : "");
  } else if (device) {
    bar.classList.add("off");
    name.classList.remove("off");
    name.textContent = entry?.key ?? device.name;
    detail.textContent = "Battery only · flash the KeyLay hook to see layers";
  } else {
    bar.classList.add("off");
    name.classList.add("off");
    name.textContent = "No current layer";
    detail.textContent = "Not connected · showing all layers";
  }

  $("batteries").replaceChildren(
    ...(device?.batteries ?? []).map((b) => {
      const el = document.createElement("div");
      const level = b.level;
      el.className = "battery" + (level == null ? "" : level < 20 ? " bad" : level < 40 ? " warn" : "");
      el.innerHTML = `<span class="label"></span><span class="value"></span><span class="track"><span class="fill"></span></span>`;
      el.querySelector(".label").textContent = b.label;
      el.querySelector(".value").textContent = level == null ? "-" : `${level}%`;
      el.querySelector(".fill").style.width = `${level ?? 0}%`;
      return el;
    }),
  );
}

function renderChips(device, keyboard) {
  const layers = keyboard?.layers ?? [];
  const live = device?.layer ?? null;
  $("chips").replaceChildren(
    ...layers.map((n, i) => {
      const chip = document.createElement("button");
      chip.className = "chip";
      chip.textContent = n;
      chip.title = `Layer ${i}: ${n}`;
      chip.style.setProperty("--chip", layerColor(state.snapshot, i));
      if (i === live) chip.classList.add("active");
      else if (device && device.layerMask & (1 << i)) chip.classList.add("on");
      if (i === state.preview) chip.classList.add("preview");
      chip.addEventListener("click", () => {
        state.preview = state.preview === i || i === live ? null : i;
        render();
      });
      return chip;
    }),
  );

  const banner = $("preview");
  banner.hidden = state.preview == null;
  if (state.preview != null) {
    $("preview-text").textContent = `Previewing ${layers[state.preview] ?? "layer " + state.preview}`;
    $("preview-end").textContent = device?.layer != null ? "Follow the keyboard" : "Done";
  }
}

function ago(ms) {
  const s = Math.max(0, (Date.now() - ms) / 1000);
  if (s < 45) return "just now";
  if (s < 90 * 60) return `${Math.max(1, Math.round(s / 60))} min ago`;
  if (s < 36 * 3600) return `${Math.round(s / 3600)} h ago`;
  return `${Math.round(s / 86400)} days ago`;
}

function renderFooter(snapshot, keyboard) {
  const source = keyboard?.source ?? "";
  $("source-icon").innerHTML = sourceIcon(source);
  const checked = $("checked");
  const error = snapshot.configError ?? keyboard?.keymapError ?? null;
  checked.classList.toggle("error", Boolean(error));
  checked.title = error ?? source;
  if (error) checked.textContent = error;
  else if (!keyboard) checked.textContent = "Not in the config";
  else if (keyboard.checkedAt) checked.textContent = `Keymap checked ${ago(keyboard.checkedAt)}`;
  else checked.textContent = keyboard.hasKeymap ? "Keymap from the saved copy" : "No keymap";
}

async function renderSheet(keyboard, device) {
  const sheet = $("sheet");
  // The revision changes when the app downloads a newer SVG, which redraws the sheet.
  const key = keyboard ? `${keyboard.name}#${keyboard.revision}#${keyboard.hasKeymap}` : null;

  if (state.sheetFor !== key) {
    state.sheetFor = key;
    state.svg = null;
    state.shown = null;
    state.groups = [];
    state.boxes = [];
    state.labels = [];
    const text = keyboard?.hasKeymap ? await invoke("get_keymap_svg", { keyboard: keyboard.name }) : null;
    if (state.sheetFor !== key) return; // another keyboard was picked meanwhile

    if (!text) {
      const p = document.createElement("p");
      p.className = "empty";
      p.textContent = keyboard
        ? 'No keymap. Set "keymap" in the config to a keymap-drawer SVG.'
        : "Add this keyboard to the config to see its keymap.";
      sheet.replaceChildren(p);
      fit("all");
      return;
    }
    sheet.innerHTML = text;
    const svg = sheet.querySelector("svg");
    svg.removeAttribute("width");
    svg.removeAttribute("height");
    state.svg = svg;
    state.fullViewBox = svg.getAttribute("viewBox");
    state.groups = [...svg.children].filter(
      (el) => el.tagName.toLowerCase() === "g" && [...el.classList].some((c) => c.startsWith("layer-")),
    );
    state.groups.forEach((g, i) => g.style.setProperty("--layer", layerColor(state.snapshot, i)));
    // Measure while every layer and label is visible; hidden elements have no box.
    document.body.dataset.mode = "all";
    state.boxes = state.groups.map((g) => box(g, (el) => !el.matches("text.label")));
    state.labels = state.groups.map((g) => box(g, () => true));
  }
  showLayer(device?.layer ?? null);
}

// Bounding box of a layer group's children that pass `keep`, in SVG coordinates
function box(group, keep) {
  const g = translation(group);
  let x0 = Infinity,
    y0 = Infinity,
    x1 = -Infinity,
    y1 = -Infinity;
  for (const el of group.children) {
    if (!keep(el)) continue;
    const b = el.getBBox();
    const t = translation(el);
    x0 = Math.min(x0, g.x + t.x + b.x);
    y0 = Math.min(y0, g.y + t.y + b.y);
    x1 = Math.max(x1, g.x + t.x + b.x + b.width);
    y1 = Math.max(y1, g.y + t.y + b.y + b.height);
  }
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

function translation(el) {
  const m = el.transform?.baseVal?.consolidate()?.matrix;
  return { x: m?.e ?? 0, y: m?.f ?? 0 };
}

// live: the keyboard's active layer, or null when it is not connected. A keyboard that is not
// connected has no current layer, so it shows every layer with none highlighted.
function showLayer(live) {
  const connected = live != null;
  const mode = connected ? state.mode : "all";
  document.body.dataset.mode = mode;
  for (const button of document.querySelectorAll(".toggle button")) {
    button.setAttribute("aria-pressed", String(button.dataset.mode === mode));
    button.disabled = !connected;
  }
  const { svg, groups, boxes, labels } = state;
  if (!svg || groups.length === 0) return fit(mode);

  const target = state.preview ?? live; // the layer to show or scroll to
  const index = target == null ? -1 : Math.min(target, groups.length - 1);
  const current = mode === "current";
  groups.forEach((g, i) => {
    g.classList.toggle("kl-active", connected && i === Math.min(live, groups.length - 1));
    g.style.display = current && i !== index ? "none" : "";
  });

  if (current) {
    const b = boxes[index];
    const pad = 4;
    svg.setAttribute("viewBox", `${b.x - pad} ${b.y - pad} ${b.w + 2 * pad} ${b.h + 2 * pad}`);
    $("sheet").scrollTop = 0;
  } else {
    svg.setAttribute("viewBox", state.fullViewBox);
    const key = `all:${index}`;
    if (state.shown !== key) {
      // Jump on the first draw, glide when the layer changes; wait a frame so the new
      // viewBox has given the sheet its full height
      const behavior = state.shown?.startsWith("all:") ? "smooth" : "instant";
      const scroll = () => {
        if (index < 0) return $("sheet").scrollTo({ top: 0, behavior });
        const [, top, width] = state.fullViewBox.split(/\s+/).map(Number);
        const scale = svg.clientWidth / width;
        $("sheet").scrollTo({ top: (labels[index].y - top) * scale, behavior });
      };
      requestAnimationFrame(() => requestAnimationFrame(scroll));
    }
  }
  state.shown = `${mode}:${index}`;
  fit(mode);
}

// Size the window to its content: the whole layer in "Current", up to 640 px in "All".
let fitQueued = false;
function fit(mode = state.mode) {
  if (fitQueued) return;
  fitQueued = true;
  requestAnimationFrame(() => {
    fitQueued = false;
    const sheet = $("sheet");
    const chrome = document.documentElement.clientHeight - sheet.clientHeight;
    // The drawing's own height: the sheet box stretches to the window, so its scrollHeight
    // never drops below the current size and the window could only grow.
    let content = 0;
    for (const el of sheet.children) {
      const style = getComputedStyle(el);
      content += el.getBoundingClientRect().height + parseFloat(style.marginTop) + parseFloat(style.marginBottom);
    }
    const natural = Math.ceil(chrome + content) + 2; // + the panel border
    // "All" grows up to the height the user last gave the window (640 px before that)
    const height = mode === "current" ? natural : Math.min(natural, state.snapshot?.popoverMaxHeight ?? 640);
    invoke("fit_popover", { height });
  });
}

function closeMenu() {
  $("menu").hidden = true;
  $("switcher").setAttribute("aria-expanded", "false");
}

$("switcher").addEventListener("click", (e) => {
  e.stopPropagation();
  const menu = $("menu");
  menu.hidden = !menu.hidden;
  $("switcher").setAttribute("aria-expanded", String(!menu.hidden));
});
document.addEventListener("click", (e) => {
  if (!$("menu").hidden && !e.target.closest(".switcher-wrap")) closeMenu();
});

for (const button of document.querySelectorAll(".toggle button")) {
  button.addEventListener("click", () => {
    state.mode = button.dataset.mode;
    write("mode", state.mode);
    render();
  });
}

$("preview-end").addEventListener("click", () => {
  state.preview = null;
  render();
});
$("pin").addEventListener("click", () => invoke("set_pinned", { pinned: !state.snapshot?.pinned }));
$("close").addEventListener("click", () => invoke("hide_popover"));
$("config").addEventListener("click", () => invoke("open_settings"));
$("reload").addEventListener("click", async () => {
  const button = $("reload");
  if (button.classList.contains("spinning")) return;
  button.classList.add("spinning");
  try {
    await invoke("reload");
  } finally {
    button.classList.remove("spinning");
  }
});

document.addEventListener("keydown", (e) => {
  if (e.key !== "Escape") return;
  if (!$("menu").hidden) closeMenu();
  else invoke("hide_popover");
});

// "checked 2 min ago" keeps counting while the popover is open
setInterval(() => state.snapshot && renderFooter(state.snapshot, entriesOf(state.snapshot).find((e) => e.key === state.selected)?.keyboard ?? null), 30000);

listen("snapshot", (e) => {
  state.snapshot = e.payload;
  render();
});
// The payload names the keyboard whose KeyLay key opened the popover, if any
listen("popover-shown", (e) => {
  state.userPicked = Boolean(e.payload);
  if (e.payload) state.selected = e.payload;
  state.preview = null;
  state.shown = null; // scroll to the current layer again
  closeMenu();
  render();
});

invoke("get_snapshot").then((snapshot) => {
  state.snapshot = snapshot;
  render();
});
