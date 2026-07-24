# FCDP v0.1 — Packet framing draft

Status: **draft; intentionally not stable**.

FCDP runs over UDP. Implementations must limit datagrams to 1200 bytes by default and must never rely on IP fragmentation. This document describes only the unencrypted M1 framing layer; it is not a security or session specification.

## Header

The current M0 implementation uses a fixed 37-byte header, followed by the declared payload.

| Bytes | Field |
| ---: | --- |
| 0–1 | magic: `FC` |
| 2 | version: `1` |
| 3 | packet type |
| 4 | flags |
| 5 | reserved (zero) |
| 6–13 | session ID (u64, big endian) |
| 14–15 | stream ID (u16) |
| 16–17 | epoch (u16) |
| 18–21 | sequence number (u32) |
| 22–25 | frame ID (u32) |
| 26–27 | fragment index (u16) |
| 28–29 | fragment count (u16) |
| 30 | priority, 0–3 (0 highest) |
| 31–32 | deadline in milliseconds (u16) |
| 33–34 | payload length (u16) |
| 35–36 | header CRC-16/CCITT-FALSE (u16) |

The header layout is intentionally marked draft until version negotiation and
cross-language test vectors are published. Do not claim compatibility before
those test vectors are available.

## Processing requirements

- Integers use network byte order.
- Receivers reject unknown versions, invalid CRCs, invalid fragment indices, invalid priorities, and payload-length mismatches before media processing.
- `deadline_ms` is a relative expiration budget. Senders must not enqueue a packet after its deadline, and receivers may discard incomplete frames after their deadline.
- Packet sequence numbers wrap modulo `u32`; replay prevention is not defined by this draft.

## Security notice

The M0 framing crate does not encrypt or authenticate media. CRC only detects accidental corruption. A future session specification will require an audited key exchange and AEAD before any networked release.
