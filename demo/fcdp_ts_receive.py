#!/usr/bin/env python3
"""Receive a live FCDP byte stream and write it to stdout in order.

This is the receiver-side counterpart to `fluxcast-cli publish-ts`. It uses the
FluxCast Python SDK to decode FCDP v0.1 datagrams, reorders by sequence number,
and skips past unrecoverable gaps so a downstream demuxer keeps flowing. The
ordered bytes (an MPEG-TS from the publisher) are meant to be piped into ffmpeg.
"""
import os
import socket
import sys

# Support both the in-repo layout (sdk/python/fluxcast/fcdp.py) and a flat
# deployment where fcdp.py sits next to this script.
_here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, _here)
sys.path.insert(0, os.path.join(_here, "..", "sdk", "python"))
try:
    from fluxcast.fcdp import decode  # noqa: E402
except ImportError:
    from fcdp import decode  # noqa: E402


def main() -> int:
    if len(sys.argv) != 2:
        sys.stderr.write("usage: fcdp_ts_receive.py <bind-host:port>\n")
        return 2
    host, _, port = sys.argv[1].rpartition(":")
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 4 << 20)
    sock.bind((host or "0.0.0.0", int(port)))
    sys.stderr.write(f"fcdp_ts_receive: listening on {host or '0.0.0.0'}:{port}\n")
    sys.stderr.flush()

    out = sys.stdout.buffer
    pending: dict[int, bytes] = {}
    nxt: int | None = None
    while True:
        datagram, _ = sock.recvfrom(1500)
        try:
            header, payload = decode(datagram)
        except ValueError:
            continue
        seq = header.sequence
        if nxt is None:
            nxt = seq
        pending.setdefault(seq, payload)

        # A stalled gap: jump to the oldest buffered chunk so we do not freeze.
        if len(pending) > 128 and nxt not in pending:
            nxt = min(pending)

        while nxt in pending:
            out.write(pending.pop(nxt))
            out.flush()
            nxt = (nxt + 1) & 0xFFFFFFFF
    # unreachable


if __name__ == "__main__":
    raise SystemExit(main())
