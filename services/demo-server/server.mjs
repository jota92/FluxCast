import { createServer } from "node:https";
import { randomUUID } from "node:crypto";
import { createReadStream, promises as fs } from "node:fs";
import { dirname, extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";
import QRCode from "qrcode";

const directory = dirname(fileURLToPath(import.meta.url));
const root = normalize(join(directory, "../.."));
const studio = join(root, "apps/demo-studio/index.html");
const senderDist = join(root, "apps/sender-web/dist");
const port = Number(process.env.PORT ?? 3030);
const origin = process.env.FLEXCAST_DEMO_ORIGIN;
const edgeUrl = process.env.FLEXCAST_EDGE_URL;
const previewUrl = process.env.FLEXCAST_EDGE_PREVIEW_URL ?? "http://127.0.0.1:3031/preview.rgba";
const certificate = process.env.FLEXCAST_TLS_CERT;
const key = process.env.FLEXCAST_TLS_KEY;
const sessions = new Map();

if (!origin || !edgeUrl || !certificate || !key) {
  console.error("Set FLEXCAST_DEMO_ORIGIN, FLEXCAST_EDGE_URL, FLEXCAST_TLS_CERT, and FLEXCAST_TLS_KEY. See docs/demo.md.");
  process.exit(1);
}
const contentTypes = { ".css": "text/css; charset=utf-8", ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".svg": "image/svg+xml", ".woff2": "font/woff2" };
function send(response, status, body, type = "text/plain; charset=utf-8") {
  response.writeHead(status, { "Content-Type": type, "Cache-Control": "no-store" });
  response.end(body);
}
function joinUrl(token) {
  const url = new URL(`/j/${token}`, origin);
  url.searchParams.set("edge", edgeUrl);
  url.searchParams.set("invite", token);
  return url.toString();
}
async function staticSender(pathname, response) {
  const relative = pathname.replace(/^\/assets\//, "");
  const file = normalize(join(senderDist, "assets", relative));
  if (!file.startsWith(`${senderDist}/assets/`)) return send(response, 403, "Forbidden");
  try {
    const info = await fs.stat(file);
    if (!info.isFile()) return send(response, 404, "Not found");
    response.writeHead(200, { "Content-Type": contentTypes[extname(file)] ?? "application/octet-stream", "Cache-Control": "public, max-age=3600" });
    createReadStream(file).pipe(response);
  } catch { send(response, 404, "Not found"); }
}
const server = createServer({ cert: await fs.readFile(certificate), key: await fs.readFile(key) }, async (request, response) => {
  const requestUrl = new URL(request.url ?? "/", origin);
  if (request.method === "GET" && requestUrl.pathname === "/") {
    response.writeHead(200, { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-store" });
    return createReadStream(studio).pipe(response);
  }
  if (request.method === "POST" && requestUrl.pathname === "/api/demo/sessions") {
    const token = randomUUID(); sessions.set(token, Date.now());
    return send(response, 201, JSON.stringify({ token, joinUrl: joinUrl(token), expiresAt: new Date(Date.now() + 15 * 60_000).toISOString() }), "application/json; charset=utf-8");
  }
  if (request.method === "GET" && requestUrl.pathname.startsWith("/api/demo/qr/")) {
    const token = requestUrl.pathname.slice("/api/demo/qr/".length);
    if (!sessions.has(token)) return send(response, 404, "Session not found");
    const svg = await QRCode.toString(joinUrl(token), { type: "svg", margin: 1, width: 360, errorCorrectionLevel: "M" });
    return send(response, 200, svg, "image/svg+xml");
  }
  if (request.method === "GET" && requestUrl.pathname === "/api/demo/preview.rgba") {
    try {
      const upstream = await fetch(previewUrl, { cache: "no-store" });
      if (!upstream.ok) return send(response, 503, "Edge preview unavailable");
      const bytes = new Uint8Array(await upstream.arrayBuffer());
      return send(response, 200, bytes, "application/octet-stream");
    } catch { return send(response, 503, "Edge preview unavailable"); }
  }
  if (request.method === "GET" && requestUrl.pathname.startsWith("/j/")) {
    const token = requestUrl.pathname.slice(3);
    if (!sessions.has(token)) return send(response, 404, "Session not found");
    response.writeHead(200, { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-store" });
    return createReadStream(join(senderDist, "index.html")).pipe(response);
  }
  if (request.method === "GET" && requestUrl.pathname.startsWith("/assets/")) return staticSender(requestUrl.pathname, response);
  return send(response, 404, "Not found");
});
server.listen(port, "0.0.0.0", () => console.log(`FlexCast demo studio: ${origin} (listening on ${port})`));
