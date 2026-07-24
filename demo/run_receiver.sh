#!/usr/bin/env bash
# Receives the live FluxCast/FCDP stream and remuxes it to HLS (no re-encode).
# Usage: run_receiver.sh [bind-host:port]
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
bind="${1:-0.0.0.0:19300}"
hls_dir="${FLUXCAST_HLS_DIR:-/tmp/fluxcast-hls}"
mkdir -p "$hls_dir"
rm -f "$hls_dir"/*.ts "$hls_dir"/*.m3u8 2>/dev/null || true

echo "FluxCast receiver: FCDP on $bind -> HLS in $hls_dir" >&2
python3 "$here/fcdp_ts_receive.py" "$bind" \
  | ffmpeg -hide_banner -loglevel warning -i - \
      -c copy -f hls -hls_time 1 -hls_list_size 6 \
      -hls_flags delete_segments+omit_endlist \
      -hls_segment_filename "$hls_dir/seg_%05d.ts" \
      "$hls_dir/stream.m3u8"
