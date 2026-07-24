#!/usr/bin/env bash
# End-to-end browser-gateway check: a WebSocket client (as a browser would)
# sends a canonical FCDP datagram through the gateway to a native UDP receiver.
# Requires Node 22+ (global WebSocket) and a built fluxcast-cli.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
port=19207
gwport=8087
token=$(openssl rand -hex 16)
work=$(mktemp -d)
recv_pid=''
gw_pid=''
cleanup() {
  [ -n "$gw_pid" ] && kill "$gw_pid" 2>/dev/null || true
  [ -n "$recv_pid" ] && kill "$recv_pid" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

cargo build -q -p fluxcast-cli
./target/debug/fluxcast-cli receive "127.0.0.1:${port}" > "$work/recv.log" 2>&1 & recv_pid=$!
disown "$recv_pid" 2>/dev/null || true
FLUXCAST_GATEWAY_TOKEN="$token" FLUXCAST_UDP_PEER="127.0.0.1:${port}" PORT="$gwport" \
  node gateway/fluxcast-gateway.mjs > "$work/gw.log" 2>&1 & gw_pid=$!
disown "$gw_pid" 2>/dev/null || true
sleep 1.2

# The HTML must be served even with a query string (regression: it 404'd on /?token=).
code=$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:${gwport}/?token=${token}")
[ "$code" = "200" ] || { echo "gateway did not serve HTML (HTTP $code)"; exit 1; }

node - "$gwport" "$token" "$root/spec/test-vectors.json" <<'JS'
const [,, port, token, vectors] = process.argv;
import('node:fs').then(async ({ readFileSync }) => {
  const data = JSON.parse(readFileSync(vectors));
  const v = data.vectors.find(v => v.name === 'media_minimal');
  const bytes = Uint8Array.from(v.packet_hex.match(/../g).map(h => parseInt(h, 16)));
  const ws = new WebSocket(`ws://127.0.0.1:${port}/fcdp?token=${token}`);
  ws.binaryType = 'arraybuffer';
  ws.onopen = () => { ws.send(bytes); setTimeout(() => { ws.close(); process.exit(0); }, 400); };
  ws.onerror = (e) => { console.error('ws error', e.message ?? e); process.exit(1); };
  setTimeout(() => { console.error('ws timeout'); process.exit(1); }, 4000);
});
JS

sleep 0.4
if grep -q 'opus-or-h264' "$work/recv.log"; then
  echo "Browser gateway round trip passed: WebSocket -> gateway -> UDP -> receiver"
else
  echo "gateway round trip FAILED; receiver log:"; cat "$work/recv.log"; exit 1
fi
