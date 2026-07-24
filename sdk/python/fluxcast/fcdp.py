"""Strict FCDP v0.1 header encoding and decoding."""
from dataclasses import dataclass
import struct

HEADER_LEN = 37
MAX_DATAGRAM = 1200

def _crc16(data: bytes) -> int:
    value = 0xFFFF
    for byte in data:
        value ^= byte << 8
        for _ in range(8):
            value = ((value << 1) ^ 0x1021) & 0xFFFF if value & 0x8000 else (value << 1) & 0xFFFF
    return value

@dataclass(frozen=True)
class FcdpHeader:
    packet_type: int = 3
    flags: int = 0
    session_id: int = 1
    stream_id: int = 1
    epoch: int = 0
    sequence: int = 1
    frame_id: int = 1
    fragment_index: int = 0
    fragment_count: int = 1
    priority: int = 0
    deadline_ms: int = 1000

    def _without_crc(self, payload_length: int) -> bytes:
        if not 0 <= self.priority <= 3 or not 0 <= self.fragment_index < self.fragment_count:
            raise ValueError("invalid FCDP priority or fragment range")
        return b"FC" + bytes((1, self.packet_type, self.flags, 0)) + struct.pack(
            ">QHHIIHHBHH", self.session_id, self.stream_id, self.epoch, self.sequence,
            self.frame_id, self.fragment_index, self.fragment_count, self.priority,
            self.deadline_ms, payload_length)

def encode(header: FcdpHeader, payload: bytes) -> bytes:
    if len(payload) + HEADER_LEN > MAX_DATAGRAM:
        raise ValueError("FCDP datagram exceeds 1200-byte budget")
    raw = header._without_crc(len(payload))
    return raw + struct.pack(">H", _crc16(raw)) + payload

def decode(packet: bytes) -> tuple[FcdpHeader, bytes]:
    if len(packet) < HEADER_LEN or packet[:2] != b"FC" or packet[2] != 1:
        raise ValueError("invalid FCDP header")
    raw = packet[:35]
    if _crc16(raw) != struct.unpack(">H", packet[35:37])[0]:
        raise ValueError("invalid FCDP header CRC")
    values = struct.unpack(">QHHIIHHBHH", raw[6:])
    payload = packet[37:]
    if values[-1] != len(payload): raise ValueError("FCDP payload length mismatch")
    header = FcdpHeader(packet_type=raw[3], flags=raw[4], session_id=values[0], stream_id=values[1], epoch=values[2], sequence=values[3], frame_id=values[4], fragment_index=values[5], fragment_count=values[6], priority=values[7], deadline_ms=values[8])
    header._without_crc(len(payload))
    return header, payload
