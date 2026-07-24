#!/usr/bin/env python3
"""Send a valid FCDP v0.1 test access unit without any third-party package."""
import socket
import struct
import sys

HEADER_LEN = 37

def crc16(data: bytes) -> int:
    value = 0xFFFF
    for byte in data:
        value ^= byte << 8
        for _ in range(8):
            value = ((value << 1) ^ 0x1021) & 0xFFFF if value & 0x8000 else (value << 1) & 0xFFFF
    return value

def packet(payload: bytes, sequence: int = 1, frame: int = 1) -> bytes:
    if len(payload) > 1163:
        raise ValueError("sample supports one FCDP fragment only")
    header = b"FC" + bytes([1, 3, 0, 0])
    header += struct.pack(">QHHIIHHBHH", 1, 1, 1, sequence, frame, 0, 1, 0, 1000, len(payload))
    return header + struct.pack(">H", crc16(header)) + payload

if __name__ == "__main__":
    host, port, text = sys.argv[1], int(sys.argv[2]), sys.argv[3]
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.sendto(packet(text.encode()), (host, port))
