#!/usr/bin/env bash
# Verifies FCDP H.264 -> HLS -> Flask with a generated video stream.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
task_dir=$(mktemp -d)
receiver_pid=''
flask_pid=''
cleanup() {
  [ -n "$receiver_pid" ] && kill "$receiver_pid" 2>/dev/null || true
  [ -n "$flask_pid" ] && kill "$flask_pid" 2>/dev/null || true
  wait "$receiver_pid" 2>/dev/null || true
  wait "$flask_pid" 2>/dev/null || true
  rm -rf "$task_dir"
}
trap cleanup EXIT

command -v ffmpeg >/dev/null
command -v curl >/dev/null
cargo build -q -p fluxcast-cli
ffmpeg -hide_banner -loglevel error -f lavfi -i testsrc2=size=160x90:rate=10 -t 3 \
  -c:v libx264 -preset ultrafast -g 10 -pix_fmt yuv420p -f h264 -y "$task_dir/input.h264"

FLUXCAST_HLS_DIR="$task_dir/hls" bash "$root/demo/run_iphone_receiver.sh" 127.0.0.1:19120 >"$task_dir/receiver.log" 2>&1 & receiver_pid=$!
FLUXCAST_HLS_DIR="$task_dir/hls" PORT=18000 python3 "$root/demo/app.py" >"$task_dir/flask.log" 2>&1 & flask_pid=$!
sleep 1
"$root/target/debug/fluxcast-cli" send-h264 127.0.0.1:19120 "$task_dir/input.h264"

for _ in {1..10}; do
  if [ -s "$task_dir/hls/stream.m3u8" ]; then break; fi
  sleep 1
done
if [ ! -s "$task_dir/hls/stream.m3u8" ]; then
  cat "$task_dir/receiver.log" >&2
  exit 1
fi
curl --fail --silent http://127.0.0.1:18000/ | rg -q 'FluxCast カメラデモ'
curl --fail --silent http://127.0.0.1:18000/hls/stream.m3u8 | rg -q '#EXTM3U'
echo "iPhone FCDP H.264 -> HLS -> Flask round trip passed"
