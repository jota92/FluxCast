// Send a valid FCDP v0.1 test access unit using only Node's standard library.
import dgram from "node:dgram";

function crc16(buffer) {
  let value = 0xffff;
  for (const byte of buffer) {
    value ^= byte << 8;
    for (let bit = 0; bit < 8; bit += 1) value = value & 0x8000 ? ((value << 1) ^ 0x1021) & 0xffff : (value << 1) & 0xffff;
  }
  return value;
}

function packet(text) {
  const payload = Buffer.from(text, "utf8");
  if (payload.length > 1163) throw new Error("sample supports one FCDP fragment only");
  const header = Buffer.alloc(35);
  header.write("FC", 0, "ascii"); header[2] = 1; header[3] = 3;
  header.writeBigUInt64BE(1n, 6); header.writeUInt16BE(1, 14); header.writeUInt16BE(1, 16);
  header.writeUInt32BE(1, 18); header.writeUInt32BE(1, 22); header.writeUInt16BE(0, 26);
  header.writeUInt16BE(1, 28); header[30] = 0; header.writeUInt16BE(1000, 31); header.writeUInt16BE(payload.length, 33);
  const checksum = Buffer.alloc(2); checksum.writeUInt16BE(crc16(header));
  return Buffer.concat([header, checksum, payload]);
}

const [host, port, text] = process.argv.slice(2);
if (!host || !port || text === undefined) throw new Error("usage: node send-fcdp.mjs <host> <port> <text>");
const socket = dgram.createSocket("udp4");
socket.send(packet(text), Number(port), host, () => socket.close());
