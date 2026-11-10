// Serves ui/ with the Tauri stand-in (stub.js) injected, so both windows run in a browser:
//   npm run ui:preview            http://localhost:8765
//   PORT=9000 npm run ui:preview
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL(".", import.meta.url));
const ui = join(here, "..", "..", "ui");
const port = Number(process.env.PORT ?? 8765);
const types = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".png": "image/png", ".svg": "image/svg+xml" };

const index = `<!doctype html><meta charset="utf-8"><title>KeyLay preview</title>
<body style="font: 14px system-ui; padding: 24px; line-height: 1.8">
<h1>KeyLay preview</h1>
<p>Cheat sheet: <a href="/index.html">connected</a> · <a href="/index.html?off">not connected</a> ·
<a href="/index.html?battery&solid">Bluetooth, solid</a></p>
<p>Settings: <a href="/settings.html">default</a> · <a href="/settings.html?battery">with batteries</a> ·
<a href="/settings.html?off">nothing connected</a></p>
<p>Commands the page sent: <code>preview.calls</code> in the console.</p>`;

createServer(async (req, res) => {
  const path = decodeURIComponent(new URL(req.url, "http://x").pathname);
  try {
    if (path === "/") {
      res.writeHead(200, { "content-type": "text/html" }).end(index);
      return;
    }
    if (path === "/__preview/stub.js") {
      res.writeHead(200, { "content-type": "text/javascript" }).end(await readFile(join(here, "stub.js")));
      return;
    }
    const file = normalize(join(ui, path));
    if (!file.startsWith(ui)) throw new Error("outside ui/");
    let body = await readFile(file);
    if (extname(file) === ".html") {
      body = String(body).replace('<script src="common.js">', '<script src="/__preview/stub.js"></script>\n    <script src="common.js">');
    }
    res.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream" }).end(body);
  } catch {
    res.writeHead(404).end("not found");
  }
}).listen(port, () => console.log(`KeyLay preview: http://localhost:${port}`));
