import { AtomType, encodeDatagram, encodeSurface, encodeRawSurface, REGION_COLUMNS, REGION_ROWS } from "@flexcast/protocol-types";

type FrameMessage = { type: "frame"; bitmap: ImageBitmap; frameTick: number; maxDatagram: number; budgetBytes: number; highQuality?: boolean };
type InitMessage = { type: "init"; epoch: number };
let epoch = 1; let sequence = 0; const previous = new Uint8Array(REGION_COLUMNS * REGION_ROWS * 48); const stateIds = new Uint32Array(REGION_COLUMNS * REGION_ROWS);

self.onmessage = (event: MessageEvent<FrameMessage | InitMessage>) => { if (event.data.type === "init") { epoch = event.data.epoch; sequence = 0; previous.fill(0); stateIds.fill(0); return; } void process(event.data); };
async function process(message: FrameMessage): Promise<void> {
  const canvas = new OffscreenCanvas(1920, 1080); const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) return; context.drawImage(message.bitmap, 0, 0, 1920, 1080); message.bitmap.close();
  const pixels = context.getImageData(0, 0, 1920, 1080).data; const candidates: { score: number; bytes: Uint8Array; region: number; luma: Uint8Array; }[] = [];
  for (let region = 0; region < REGION_COLUMNS * REGION_ROWS; region++) { const luma = new Uint8Array(48); const ca = new Int8Array(12); const cb = new Int8Array(12); const row = Math.floor(region / REGION_COLUMNS); const col = region % REGION_COLUMNS;
    for (let gy = 0; gy < 6; gy++) for (let gx = 0; gx < 8; gx++) { let sum = 0; for (let sy = 0; sy < 4; sy++) for (let sx = 0; sx < 4; sx++) { const pixel = (((row * 24 + gy * 4 + sy) * 1920) + col * 32 + gx * 4 + sx) * 4; const r = pixels[pixel] ?? 0, g = pixels[pixel + 1] ?? 0, b = pixels[pixel + 2] ?? 0; sum += (r + 2 * g + b) >> 2; } luma[gy * 8 + gx] = sum >> 4; }
    for (let gy = 0; gy < 3; gy++) for (let gx = 0; gx < 4; gx++) { let a = 0, b = 0; for (let sy = 0; sy < 8; sy++) for (let sx = 0; sx < 8; sx++) { const pixel = (((row * 24 + gy * 8 + sy) * 1920) + col * 32 + gx * 8 + sx) * 4; const r = pixels[pixel] ?? 0, g = pixels[pixel + 1] ?? 0, blue = pixels[pixel + 2] ?? 0; a += r - g; b += blue - g; } ca[gy * 4 + gx] = Math.round(a / 64); cb[gy * 4 + gx] = Math.round(b / 64); }
    let delta = 0; const offset = region * 48; for (let i = 0; i < 48; i++) delta += Math.abs((luma[i] ?? 0) - (previous[offset + i] ?? 0));
    const ageScore = message.frameTick % 15 === region % 15 ? 20 : 0; if (delta < 80 && ageScore === 0) continue;
    const nextState = (stateIds[region] ?? 0) + 1; const raw = new Uint8Array(32 * 24 * 3); if (message.highQuality) { for (let y = 0; y < 24; y++) for (let x = 0; x < 32; x++) { const source = (((row * 24 + y) * 1920) + col * 32 + x) * 4; const target = (y * 32 + x) * 3; raw[target] = pixels[source] ?? 0; raw[target + 1] = pixels[source + 1] ?? 0; raw[target + 2] = pixels[source + 2] ?? 0; } } const payload = message.highQuality ? encodeRawSurface(raw) : encodeSurface(luma, ca, cb); const header = { atomType: AtomType.Surface, flags: 1, sessionEpoch: epoch, atomSequence: ++sequence, frameTick: message.frameTick, regionId: region, fragmentIndex: 0, fragmentCount: 1, stateId: nextState, baseStateId: 0, captureTimeMs: Math.floor(performance.now()), ttlMs: 120 };
    const packet = encodeDatagram(header, payload); if (packet.byteLength <= message.maxDatagram) candidates.push({ score: delta + ageScore, bytes: packet, region, luma });
  }
  candidates.sort((a, b) => b.score - a.score); let used = 0; const selected: ArrayBuffer[] = []; for (const candidate of candidates) { if (used + candidate.bytes.byteLength > message.budgetBytes) continue; used += candidate.bytes.byteLength; previous.set(candidate.luma, candidate.region * 48); stateIds[candidate.region] = (stateIds[candidate.region] ?? 0) + 1; const transferable = new Uint8Array(candidate.bytes); selected.push(transferable.buffer as ArrayBuffer); }
  (self as unknown as { postMessage: (message: unknown, transfer: Transferable[]) => void }).postMessage({ type: "atoms", packets: selected, generated: candidates.length, sentBytes: used }, selected);
}
