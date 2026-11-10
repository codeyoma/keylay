// Shared by the cheat sheet (app.js) and the settings window (settings.js).

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);

// Lucide icons (stroke paths)
const ICONS = {
  ble: '<path d="m7 7 10 10-5 5V2l5 5L7 17"/>',
  bleOff: '<path d="m17 17-5 5V12l-5 5"/><path d="m2 2 20 20"/><path d="M14.5 9.5 17 7l-5-5v4.5"/>',
  usb: '<circle cx="10" cy="7" r="1"/><circle cx="4" cy="20" r="1"/><path d="M4.7 19.3 19 5"/><path d="m21 3-3 1 2 2Z"/><path d="M9.26 7.68 5 12l2 5"/><path d="m10 14 5 2 3.5-3.5"/><path d="m18 12 1-1 1 1-1 1Z"/>',
  github: '<path d="M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4"/><path d="M9 18c-4.51 2-5-2-7-2"/>',
  file: '<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/>',
  globe: '<circle cx="12" cy="12" r="10"/><path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/><path d="M2 12h20"/>',
  plus: '<path d="M5 12h14"/><path d="M12 5v14"/>',
};
const icon = (name) => `<svg class="icon" viewBox="0 0 24 24" aria-hidden="true">${ICONS[name]}</svg>`;

// The icon for a keymap source: GitHub, another web address, or a file
const sourceIcon = (source) => icon(/github/.test(source) ? "github" : /^https?:/.test(source) ? "globe" : "file");

// The app sends the layer colours (the same as the tray badge) with every snapshot
const layerColor = (snapshot, i) => {
  const colors = snapshot?.layerColors ?? ["#8A8F98"];
  return colors[i % colors.length];
};

// The connected device for a keyboard: `key` is its config name, or the device name for a
// keyboard that is not in the config
const deviceFor = (snapshot, key) => snapshot?.devices.find((d) => d.key === key) ?? null;

// Drag to reorder, with pointer events (HTML drag and drop is not reliable in the webview).
// While dragged, the item follows the pointer and a gap opens where it would land, in
// whichever of `zones()` is nearest. A press that does not move is a click.
//   handle: a selector inside the item to drag by (default: the whole item)
//   onDrop(): the item is in its new place in the DOM; read the order from there
//   onClick(): pressed and released without moving
function draggable(item, { zones, handle, onDrop, onClick }) {
  const grip = handle ? item.querySelector(handle) : item;
  grip.style.touchAction = "none";
  grip.addEventListener("pointerdown", (down) => {
    if (down.button !== 0) return;
    down.preventDefault();
    const start = item.getBoundingClientRect();
    const offset = { x: down.clientX - start.left, y: down.clientY - start.top };
    let gap = null;

    const move = (e) => {
      if (!gap) {
        if (Math.hypot(e.clientX - down.clientX, e.clientY - down.clientY) < 4) return;
        const r = item.getBoundingClientRect();
        gap = document.createElement("div");
        gap.className = `${item.className} drag-gap`;
        gap.style.width = `${r.width}px`;
        gap.style.height = `${r.height}px`;
        item.replaceWith(gap);
        item.classList.add("drag-lifted");
        item.style.width = `${r.width}px`;
        document.body.append(item);
      }
      item.style.left = `${e.clientX - offset.x}px`;
      item.style.top = `${e.clientY - offset.y}px`;
      const distance = (z) => {
        const r = z.getBoundingClientRect();
        return Math.abs(e.clientY - Math.min(Math.max(e.clientY, r.top), r.bottom));
      };
      const zone = zones().reduce((best, z) => (distance(z) < distance(best) ? z : best));
      const before = [...zone.children].find((el) => {
        if (el === gap) return false;
        const r = el.getBoundingClientRect();
        return e.clientX < r.left + r.width / 2;
      });
      if (before) zone.insertBefore(gap, before);
      else zone.append(gap);
    };

    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
      if (!gap) return onClick?.();
      item.classList.remove("drag-lifted");
      item.style.left = item.style.top = item.style.width = "";
      gap.replaceWith(item);
      onDrop?.();
    };

    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
  });
}
