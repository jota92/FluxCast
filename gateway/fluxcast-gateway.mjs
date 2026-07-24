#!/usr/bin/env node
/**
 * FluxCast browser gateway: WebSocket binary frames <-> UDP FCDP datagrams.
 * It intentionally cannot decrypt FCDP. Configure a single permitted UDP
 * destination and a non-empty bearer token before exposing it on a network.
 */
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { createSocket } from 'node:dgram';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const token = process.env.FLUXCAST_GATEWAY_TOKEN;
const peer = process.env.FLUXCAST_UDP_PEER;
const port = Number(process.env.PORT ?? 8080);
if (!token || !peer) throw new Error('Set FLUXCAST_GATEWAY_TOKEN and FLUXCAST_UDP_PEER=host:port');
const separator = peer.lastIndexOf(':');
const peerHost = peer.slice(0, separator);
const peerPort = Number(peer.slice(separator + 1));
if (!peerHost || !Number.isInteger(peerPort) || peerPort < 1 || peerPort > 65535) throw new Error('FLUXCAST_UDP_PEER must be host:port');
const root = dirname(fileURLToPath(import.meta.url));

const http = createServer(async (request, response) => {
  const path = new URL(request.url, `http://${request.headers.host}`).pathname;
  if (path !== '/') { response.writeHead(404).end(); return; }
  response.writeHead(200, { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' });
  response.end(await readFile(join(root, 'index.html')));
});

http.on('upgrade', (request, socket) => {
  const url = new URL(request.url, `http://${request.headers.host}`);
  if (url.pathname !== '/fcdp' || url.searchParams.get('token') !== token || request.headers['sec-websocket-version'] !== '13') {
    socket.write('HTTP/1.1 401 Unauthorized\r\nConnection: close\r\n\r\n'); socket.destroy(); return;
  }
  const key = request.headers['sec-websocket-key'];
  if (typeof key !== 'string') { socket.destroy(); return; }
  const accept = createHash('sha1').update(`${key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`).digest('base64');
  socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
  bridge(socket);
});

function bridge(socket) {
  const udp = createSocket('udp4'); let buffered = Buffer.alloc(0);
  udp.on('message', message => socket.writable && socket.write(webSocketFrame(message)));
  socket.on('data', data => {
    buffered = Buffer.concat([buffered, data]);
    for (;;) {
      const frame = decodeFrame(buffered);
      if (!frame) return;
      buffered = buffered.subarray(frame.consumed);
      if (frame.opcode === 0x8) { socket.end(); return; }
      if (frame.opcode === 0x2 && frame.payload.length <= 1200 && frame.payload.subarray(0, 2).equals(Buffer.from('FC'))) udp.send(frame.payload, peerPort, peerHost);
    }
  });
  socket.on('close', () => udp.close()); socket.on('error', () => udp.close());
}

function decodeFrame(buffer) {
  if (buffer.length < 2) return null;
  const opcode = buffer[0] & 0x0f; const masked = (buffer[1] & 0x80) !== 0; let length = buffer[1] & 0x7f; let offset = 2;
  if (!masked || (buffer[0] & 0x80) === 0) return { opcode: 0x8, payload: Buffer.alloc(0), consumed: buffer.length };
  if (length === 126) { if (buffer.length < 4) return null; length = buffer.readUInt16BE(2); offset = 4; }
  if (length === 127 || length > 1200 || buffer.length < offset + 4 + length) return null;
  const mask = buffer.subarray(offset, offset + 4); offset += 4; const payload = Buffer.from(buffer.subarray(offset, offset + length));
  for (let index = 0; index < payload.length; index += 1) payload[index] ^= mask[index % 4];
  return { opcode, payload, consumed: offset + length };
}
function webSocketFrame(payload) {
  if (payload.length > 125) return Buffer.concat([Buffer.from([0x82, 126, payload.length >> 8, payload.length & 255]), payload]);
  return Buffer.concat([Buffer.from([0x82, payload.length]), payload]);
}
http.listen(port, '127.0.0.1', () => console.log(`FluxCast gateway on 127.0.0.1:${port}; UDP peer ${peer}`));
