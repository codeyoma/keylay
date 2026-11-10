// A stand-in for the Tauri runtime, so the cheat sheet (index.html) and the settings window
// (settings.html) run in a plain browser: `npm run ui:preview`. Loaded before common.js by
// the preview server only; the app never ships it.
//
// Query parameters:
//   off        the keyboard is not connected
//   battery    a Bluetooth keyboard with three batteries
//   solid      the solid appearance instead of glass
//
// window.preview.calls records every command the page sent (fit_popover heights, saves).

(() => {
  const params = new URLSearchParams(location.search);
  const KEYMAP = "https://raw.githubusercontent.com/codeyoma/keyboard-zmk-sweep/main/sweep_keymap.svg";
  const COLORS = ["#8A8F98", "#3B82F6", "#10B981", "#F59E0B", "#8B5CF6", "#EC4899", "#14B8A6"];
  const layerNames = (svg) => [...new Set([...svg.matchAll(/class="layer-([^"]+)"/g)].map((m) => m[1]))];
  // The keymap, and its layer names, are read once from GitHub
  const keymap = fetch(KEYMAP).then((r) => r.text());
  const layers = keymap.then(layerNames);

  const config = {
    titleFormat: "{layer} {battery}",
    layerWidth: 6,
    keymapCheckMinutes: 15,
    keylayKey: true,
    appearance: params.has("solid") ? "solid" : "glass",
    keyboards: [
      { name: "Sweep", match: "Sweep", transport: "", keymap: KEYMAP, batteryLabels: [], layerNames: [] },
    ],
  };

  const batteries = params.has("battery")
    ? [
        { label: "Central", level: 90 },
        { label: "Peripheral 1", level: 40 },
        { label: "Peripheral 2", level: 15 },
      ]
    : [];
  const device = {
    id: "preview",
    key: "Sweep",
    name: "Sweep Dongle",
    transport: params.has("battery") ? "ble" : "usb",
    keyboard: "Sweep",
    layer: 1,
    layerName: "Layer 1",
    layerMask: 0b11,
    batteries,
    status: "",
  };
  layers.then((names) => {
    device.layerName = names[1] ?? "Layer 1";
    device.status = `${params.has("battery") ? "Bluetooth" : "USB"} · ${device.layerName}`;
  });

  const snapshot = async () => {
    const names = await layers;
    return {
      devices: params.has("off") ? [] : [device],
      keyboards: config.keyboards.map((k) => ({
        name: k.name,
        revision: 1,
        source: k.keymap,
        checkedAt: Date.now(),
        layers: k.layerNames.length ? k.layerNames : names,
        hasKeymap: true,
        keymapError: null,
      })),
      pinned: true,
      configPath: "~/Library/Application Support/dev.yoma.keylay/config.json",
      configError: null,
      titleFormat: config.titleFormat,
      layerWidth: config.layerWidth,
      appearance: config.appearance,
      popoverMaxHeight: 640,
      layerColors: COLORS,
    };
  };

  const calls = [];

  const commands = {
    get_snapshot: snapshot,
    get_config: () => structuredClone(config),
    save_config: ({ config: next }) => Object.assign(config, next),
    get_keymap_svg: () => keymap,
    check_keymap: async ({ source }) => layerNames(await (await fetch(source)).text()),
    // Close to the app's rules (title.rs), good enough to see the preview change
    preview_title: ({ format }) =>
      format
        .replace("{keyboard}", device.key)
        .replace("{layer}", device.layerName.padEnd(config.layerWidth, " "))
        .replace("{battery}", " 62%"),
    get_login: () => false,
  };

  window.preview = { calls, config, device };
  window.__TAURI__ = {
    core: {
      invoke: async (cmd, args = {}) => {
        calls.push({ cmd, args });
        return commands[cmd] ? commands[cmd](args) : null;
      },
    },
    event: { listen: async () => () => {} },
  };
})();
