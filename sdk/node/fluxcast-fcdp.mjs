/** Strict, dependency-free FCDP v0.1 framing SDK for Node.js 20+. */
export const HEADER_LEN = 37;
export function crc16(bytes) { let value = 0xffff; for (const byte of bytes) { value ^= byte << 8; for (let bit = 0; bit < 8; bit += 1) value = value & 0x8000 ? ((value << 1) ^ 0x1021) & 0xffff : (value << 1) & 0xffff; } return value; }
export function encode(header, payload) {
  if (!Buffer.isBuffer(payload)) payload = Buffer.from(payload);
  if (payload.length + HEADER_LEN > 1200) throw new Error("FCDP datagram exceeds 1200-byte budget");
  if (header.priority > 3 || header.fragmentIndex >= header.fragmentCount) throw new Error("invalid FCDP fragment range");
  const raw = Buffer.alloc(35); raw.write("FC", 0, "ascii"); raw[2] = 1; raw[3] = header.packetType ?? 3; raw[4] = header.flags ?? 0;
  raw.writeBigUInt64BE(BigInt(header.sessionId ?? 1), 6); raw.writeUInt16BE(header.streamId ?? 1, 14); raw.writeUInt16BE(header.epoch ?? 0, 16); raw.writeUInt32BE(header.sequence ?? 1, 18); raw.writeUInt32BE(header.frameId ?? 1, 22); raw.writeUInt16BE(header.fragmentIndex ?? 0, 26); raw.writeUInt16BE(header.fragmentCount ?? 1, 28); raw[30] = header.priority ?? 0; raw.writeUInt16BE(header.deadlineMs ?? 1000, 31); raw.writeUInt16BE(payload.length, 33);
  const checksum = Buffer.alloc(2); checksum.writeUInt16BE(crc16(raw)); return Buffer.concat([raw, checksum, payload]);
}
export function decode(packet) { if (!Buffer.isBuffer(packet)) packet = Buffer.from(packet); if (packet.length < HEADER_LEN || packet.subarray(0, 2).toString("ascii") !== "FC" || packet[2] !== 1 || crc16(packet.subarray(0, 35)) !== packet.readUInt16BE(35)) throw new Error("invalid FCDP header"); const length = packet.readUInt16BE(33); const payload = packet.subarray(37); if (payload.length !== length) throw new Error("FCDP payload length mismatch"); return { header: { packetType: packet[3], flags: packet[4], sessionId: packet.readBigUInt64BE(6), streamId: packet.readUInt16BE(14), epoch: packet.readUInt16BE(16), sequence: packet.readUInt32BE(18), frameId: packet.readUInt32BE(22), fragmentIndex: packet.readUInt16BE(26), fragmentCount: packet.readUInt16BE(28), priority: packet[30], deadlineMs: packet.readUInt16BE(31) }, payload }; }
