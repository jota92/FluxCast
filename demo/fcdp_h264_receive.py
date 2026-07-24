#!/usr/bin/env python3
"""Reassemble FCDP H.264 access units and write Annex-B bytes to stdout.

The iPhone camera sender emits one FCDP MEDIA frame per encoded access unit,
with each frame split to the protocol's 1200-byte datagram budget. This process
only validates FCDP framing and restores complete frames; FFmpeg downstream
remuxes the resulting Annex-B H.264 as HLS for the Flask page.
"""
import os
import socket
import sys

_here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(_here, "..", "sdk", "python"))
from fluxcast.fcdp import decode  # noqa: E402


def main() -> int:
    if len(sys.argv) != 2:
        sys.stderr.write("usage: fcdp_h264_receive.py <bind-host:port>\n")
        return 2
    host, _, port = sys.argv[1].rpartition(":")
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 4 << 20)
    sock.bind((host or "0.0.0.0", int(port)))
    sys.stderr.write(f"fcdp_h264_receive: listening on {host or '0.0.0.0'}:{port}\n")
    sys.stderr.flush()

    # frame ID -> (expected fragment count, indexed payloads). A bounded window
    # prevents malformed or incomplete traffic from consuming unbounded memory.
    pending: dict[int, tuple[int, dict[int, bytes]]] = {}
    out = sys.stdout.buffer
    while True:
        datagram, _ = sock.recvfrom(1500)
        try:
            header, payload = decode(datagram)
        except ValueError:
            continue
        if header.packet_type != 3:
            continue
        count, fragments = pending.setdefault(header.frame_id, (header.fragment_count, {}))
        if count != header.fragment_count:
            pending.pop(header.frame_id, None)
            continue
        fragments.setdefault(header.fragment_index, payload)
        if len(fragments) == count:
            try:
                out.write(b"".join(fragments[index] for index in range(count)))
                out.flush()
            except KeyError:
                pass
            pending.pop(header.frame_id, None)
        if len(pending) > 120:
            pending.pop(next(iter(pending)))


if __name__ == "__main__":
    raise SystemExit(main())
