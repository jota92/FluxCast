#!/usr/bin/env bash
# Verifies every SDK encodes/decodes the canonical spec/test-vectors.json
# identically to the Rust reference. Run after any wire-format change.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
vectors="$root/spec/test-vectors.json"

echo "== Rust reference (regenerates and checks the committed vectors) =="
cargo test -q -p fluxcast-proto --test vectors

echo "== Python SDK =="
python3 - "$vectors" <<'PY'
import json, sys
sys.path.insert(0, "sdk/python")
from fluxcast.fcdp import FcdpHeader, encode, decode
data = json.load(open(sys.argv[1]))
for v in data["vectors"]:
    h = v["header"]
    header = FcdpHeader(packet_type=h["packet_type"], flags=h["flags"], session_id=h["session_id"],
        stream_id=h["stream_id"], epoch=h["epoch"], sequence=h["sequence_number"], frame_id=h["frame_id"],
        fragment_index=h["fragment_index"], fragment_count=h["fragment_count"], priority=h["priority"],
        deadline_ms=h["deadline_ms"])
    payload = bytes.fromhex(v["payload_hex"])
    packet = encode(header, payload)
    assert packet.hex() == v["packet_hex"], f'python encode mismatch: {v["name"]}'
    decoded, body = decode(packet)
    assert body == payload and decoded == header, f'python decode mismatch: {v["name"]}'
print(f'  {len(data["vectors"])} vectors matched')
PY

echo "== Node.js SDK =="
node --input-type=module - "$vectors" <<'JS'
import { readFileSync } from "node:fs";
import { encode, decode } from "./sdk/node/fluxcast-fcdp.mjs";
const data = JSON.parse(readFileSync(process.argv[2]));
for (const v of data.vectors) {
  const h = v.header;
  const header = { packetType: h.packet_type, flags: h.flags, sessionId: h.session_id, streamId: h.stream_id,
    epoch: h.epoch, sequence: h.sequence_number, frameId: h.frame_id, fragmentIndex: h.fragment_index,
    fragmentCount: h.fragment_count, priority: h.priority, deadlineMs: h.deadline_ms };
  const payload = Buffer.from(v.payload_hex, "hex");
  const packet = encode(header, payload);
  if (packet.toString("hex") !== v.packet_hex) throw new Error(`node encode mismatch: ${v.name}`);
  const { payload: body } = decode(packet);
  if (Buffer.compare(body, payload) !== 0) throw new Error(`node decode mismatch: ${v.name}`);
}
console.log(`  ${data.vectors.length} vectors matched`);
JS

echo "== Go SDK =="
(
  cd "$root/sdk/go"
  go test ./...
)

echo "== Swift SDK =="
swift test --package-path "$root/sdk/swift"

echo "== Kotlin SDK =="
if [[ "${FLUXCAST_VERIFY_KOTLIN:-0}" == "1" ]]; then
  tmp_dir=$(mktemp -d)
  trap 'rm -rf "$tmp_dir"' EXIT
  timeout 120 kotlinc "$root/sdk/kotlin/src/main/kotlin/FluxCastFcdp.kt" \
    "$root/sdk/kotlin/src/test/kotlin/VectorTest.kt" \
    -include-runtime -d "$tmp_dir/vectors.jar"
  java -jar "$tmp_dir/vectors.jar" "$vectors"
else
  echo "  skipped (set FLUXCAST_VERIFY_KOTLIN=1 to enable the local Kotlin toolchain test)"
fi

echo "All enabled SDKs reproduce the canonical FCDP v0.1 test vectors."
