#!/usr/bin/env bash
# Reproducible localhost FCDP media round trip: H.264 Annex-B and Ogg Opus.
set -euo pipefail

task_dir=$(mktemp -d)
h264_pid=''
opus_pid=''
cleanup() {
  [ -n "$h264_pid" ] && kill "$h264_pid" 2>/dev/null || true
  [ -n "$opus_pid" ] && kill "$opus_pid" 2>/dev/null || true
  wait "$h264_pid" 2>/dev/null || true
  wait "$opus_pid" 2>/dev/null || true
  rm -rf "$task_dir"
}
trap cleanup EXIT

command -v ffmpeg >/dev/null
cargo build -q -p fluxcast-cli
ffmpeg -hide_banner -loglevel error -f lavfi -i testsrc=size=160x90:rate=10 -t 1 -c:v libx264 -preset ultrafast -f h264 -y "$task_dir/input.h264"
ffmpeg -hide_banner -loglevel error -f lavfi -i sine=frequency=440:sample_rate=48000 -t 1 -c:a libopus -y "$task_dir/input.opus"

./target/debug/fluxcast-cli receive-file 127.0.0.1:19110 "$task_dir/output.h264" > "$task_dir/h264.log" 2>&1 & h264_pid=$!
./target/debug/fluxcast-cli receive-file 127.0.0.1:19111 "$task_dir/output.opus" > "$task_dir/opus.log" 2>&1 & opus_pid=$!
sleep 1
./target/debug/fluxcast-cli send-h264 127.0.0.1:19110 "$task_dir/input.h264"
./target/debug/fluxcast-cli send-opus 127.0.0.1:19111 "$task_dir/input.opus"
sleep 1
kill "$h264_pid" "$opus_pid" 2>/dev/null || true
wait "$h264_pid" "$opus_pid" 2>/dev/null || true
h264_pid=''
opus_pid=''

cmp "$task_dir/input.h264" "$task_dir/output.h264"
cmp "$task_dir/input.opus" "$task_dir/output.opus"
ffmpeg -hide_banner -loglevel error -f h264 -i "$task_dir/output.h264" -f null -
ffmpeg -hide_banner -loglevel error -i "$task_dir/output.opus" -f null -
echo "H.264 and Opus FCDP round trips passed"
