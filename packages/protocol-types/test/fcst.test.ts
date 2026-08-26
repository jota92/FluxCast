import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { AtomType, decodeDatagram, encodeDatagram, encodeSurface } from "../src/fcst.ts";

const packet = encodeDatagram({ atomType: AtomType.Surface, flags: 0, sessionEpoch: 7, atomSequence: 99, frameTick: 42, regionId: 2699, fragmentIndex: 0, fragmentCount: 1, stateId: 8, baseStateId: 7, captureTimeMs: 1234, ttlMs: 120 }, encodeSurface(new Uint8Array(48).fill(12), new Int8Array(12), new Int8Array(12)));
assert.equal(packet.byteLength, 113);
assert.deepEqual(decodeDatagram(packet).header.regionId, 2699);
assert.throws(() => decodeDatagram(packet.slice(0, -1)));
const golden = readFileSync(new URL("../../../tests/protocol/surface_001.hex", import.meta.url), "utf8").replace(/\s/g, "");
const vector = encodeDatagram({ atomType: AtomType.Surface, flags: 0, sessionEpoch: 7, atomSequence: 99, frameTick: 42, regionId: 2699, fragmentIndex: 0, fragmentCount: 1, stateId: 8, baseStateId: 7, captureTimeMs: 1234, ttlMs: 120 }, encodeSurface(new Uint8Array(48).fill(12), new Int8Array(12), new Int8Array(12)));
assert.equal(Buffer.from(vector).toString("hex"), golden);
console.log("FCST TypeScript protocol tests passed");
