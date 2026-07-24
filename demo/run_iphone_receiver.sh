#!/usr/bin/env bash
# Receives FluxCastCamera H.264/FCDP and exposes it as HLS for demo/app.py.
# Usage: run_iphone_receiver.sh [bind-host:port]
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
bind="${1:-0.0.0.0:19100}"
hls_dir="${FLUXCAST_HLS_DIR:-/tmp/fluxcast-hls}"
mkdir -p "$hls_dir"
rm -f "$hls_dir"/*.ts "$hls_dir"/*.m3u8 2>/dev/null || true

echo "FluxCast iPhone receiver: FCDP/H.264 on $bind -> HLS in $hls_dir" >&2
python3 "$here/fcdp_h264_receive.py" "$bind" \
  | ffmpeg -hide_banner -loglevel warning -analyzeduration 0 -probesize 32k \
      -f h264 -i - -an -c:v copy \
      -f hls -hls_time 1 -hls_list_size 4 \
      -hls_flags delete_segments+omit_endlist \
      -hls_segment_filename "$hls_dir/seg_%05d.ts" \
      "$hls_dir/stream.m3u8"
