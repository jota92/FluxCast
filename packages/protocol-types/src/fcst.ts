export const FCST_HEADER_BYTES = 40;
export const FCST_VERSION = 1;
export const FCST_MAGIC = [0xfc, 0x01] as const;
export const REGION_COLUMNS = 60;
export const REGION_ROWS = 45;
export const REGION_COUNT = REGION_COLUMNS * REGION_ROWS;
export const MAX_FRAGMENTS_PER_ATOM = 8;

export enum AtomType {
  Motion = 0x01,
  Surface = 0x02,
  Detail = 0x03,
  Refresh = 0x04,
  Repair = 0x05,
  GroupRepair = 0x06,
  AudioPcm = 0x20,
  Ping = 0x40,
  Pong = 0x41,
  StateDigest = 0x42,
  NetworkMetrics = 0x43,
}

export interface FcstHeader {
  atomType: AtomType;
  flags: number;
  sessionEpoch: number;
  atomSequence: number;
  frameTick: number;
  regionId: number;
  fragmentIndex: number;
  fragmentCount: number;
  stateId: number;
  baseStateId: number;
  captureTimeMs: number;
  ttlMs: number;
}

function assertInteger(value: number, min: number, max: number, name: string): void {
  if (!Number.isInteger(value) || value < min || value > max) throw new Error(`invalid ${name}`);
}

export function encodeDatagram(header: FcstHeader, payload: Uint8Array): Uint8Array {
  assertInteger(header.regionId, 0, REGION_COUNT - 1, "region id");
  assertInteger(header.fragmentCount, 1, MAX_FRAGMENTS_PER_ATOM, "fragment count");
  assertInteger(header.fragmentIndex, 0, header.fragmentCount - 1, "fragment index");
  assertInteger(header.ttlMs, 1, 60_000, "ttl");
  if (payload.byteLength > 0xffff) throw new Error("payload too large");
  const out = new Uint8Array(FCST_HEADER_BYTES + payload.byteLength);
  const view = new DataView(out.buffer);
  out[0] = FCST_MAGIC[0]; out[1] = FCST_MAGIC[1]; out[2] = FCST_VERSION; out[3] = header.atomType;
  view.setUint16(4, header.flags); view.setUint16(6, FCST_HEADER_BYTES);
  view.setUint32(8, header.sessionEpoch); view.setUint32(12, header.atomSequence);
  view.setUint32(16, header.frameTick); view.setUint16(20, header.regionId);
  out[22] = header.fragmentIndex; out[23] = header.fragmentCount;
  view.setUint32(24, header.stateId); view.setUint32(28, header.baseStateId);
  view.setUint32(32, header.captureTimeMs); view.setUint16(36, header.ttlMs);
  view.setUint16(38, payload.byteLength); out.set(payload, FCST_HEADER_BYTES);
  return out;
}

export function decodeDatagram(input: Uint8Array): { header: FcstHeader; payload: Uint8Array } {
  if (input.byteLength < FCST_HEADER_BYTES) throw new Error("truncated FCST header");
  const view = new DataView(input.buffer, input.byteOffset, input.byteLength);
  if (input[0] !== FCST_MAGIC[0] || input[1] !== FCST_MAGIC[1] || input[2] !== FCST_VERSION) throw new Error("invalid FCST magic or version");
  if (view.getUint16(6) !== FCST_HEADER_BYTES) throw new Error("unsupported header length");
  const payloadLength = view.getUint16(38);
  if (input.byteLength !== FCST_HEADER_BYTES + payloadLength) throw new Error("payload length mismatch");
  const atomType = input[3] as AtomType;
  if (!Object.values(AtomType).includes(atomType)) throw new Error("unknown atom type");
  const header: FcstHeader = { atomType, flags: view.getUint16(4), sessionEpoch: view.getUint32(8), atomSequence: view.getUint32(12), frameTick: view.getUint32(16), regionId: view.getUint16(20), fragmentIndex: input[22]!, fragmentCount: input[23]!, stateId: view.getUint32(24), baseStateId: view.getUint32(28), captureTimeMs: view.getUint32(32), ttlMs: view.getUint16(36) };
  assertInteger(header.regionId, 0, REGION_COUNT - 1, "region id");
  assertInteger(header.fragmentCount, 1, MAX_FRAGMENTS_PER_ATOM, "fragment count");
  assertInteger(header.fragmentIndex, 0, header.fragmentCount - 1, "fragment index");
  assertInteger(header.ttlMs, 1, 60_000, "ttl");
  return { header, payload: input.slice(FCST_HEADER_BYTES) };
}

export function encodeSurface(luma: Uint8Array, chromaA: Int8Array, chromaB: Int8Array, quantization = 1): Uint8Array {
  if (luma.length !== 48 || chromaA.length !== 12 || chromaB.length !== 12) throw new Error("invalid surface grid");
  const output = new Uint8Array(73); output[0] = quantization; output.set(luma, 1);
  output.set(new Uint8Array(chromaA.buffer, chromaA.byteOffset, chromaA.byteLength), 49);
  output.set(new Uint8Array(chromaB.buffer, chromaB.byteOffset, chromaB.byteLength), 61);
  return output;
}
