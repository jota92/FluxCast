# FCDP v0.1 — Packet framing draft

Status: **draft; intentionally not stable**.

FCDP runs over UDP. Implementations must limit datagrams to 1200 bytes by default and must never rely on IP fragmentation.

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
- Packet sequence numbers wrap modulo `u32`; a new epoch and session key are mandatory before nonce reuse.

## Encrypted payload profile

An encrypted packet sets flag bit 0. Its payload is ChaCha20-Poly1305
ciphertext followed by a 16-byte authentication tag. The first 35 header bytes
(through `payload length`) are AEAD associated data; therefore all routing,
priority, deadline, epoch, and sequence fields are authenticated. The CRC
remains an inexpensive accidental-corruption filter and is not a security
control.

The nonce is the 12-byte concatenation of `session_id` and `sequence_number`.
Implementations must establish a fresh key for every epoch and reject duplicate
or out-of-window sequence numbers before releasing plaintext.

## Authenticated handshake profile

The version-1 handshake is an application control exchange before encrypted
FCDP traffic. It carries `ClientHello` (136 bytes) and `ServerWelcome` (168
bytes). Both records use network byte order and must be carried intact; their
wire codecs are exposed by `fluxcast-security` for every SDK binding.

- `ClientHello` = session ID, client ephemeral X25519 public key, client
  Ed25519 public identity, and an Ed25519 signature over the labelled client
  transcript.
- `ServerWelcome` = session ID, echoed client ephemeral key, server ephemeral
  X25519 public key, server Ed25519 public identity, and a signature over the
  complete client hello plus server ephemeral key.
- Both sides pin/authorize the expected Ed25519 identity before accepting the
  exchange. A server that admits arbitrary signed identities must still apply
  an explicit authorization policy to the returned client identity.
- The two ephemeral X25519 keys feed HKDF-SHA256. The labelled full transcript
  is HKDF info, session ID is salt, and the 64-byte result is split into
  client-to-server and server-to-client ChaCha20-Poly1305 keys. This provides
  forward secrecy and prevents nonce reuse across directions.

Handshakes are not media packets and must be rate-limited, expiry-bound, and
bound to the selected ICE candidate pair by the connection layer. Key rotation
creates a new handshake and uses the negotiated epoch in subsequent FCDP
headers.
