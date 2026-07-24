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

The header layout is intentionally marked draft until version negotiation is
finalized. Canonical cross-language test vectors are published in
[`test-vectors.json`](test-vectors.json); the Rust, Python, and Node.js SDKs are
checked against them by `scripts/verify_vectors.sh`. Regenerate the file after an
intentional wire change with `FLUXCAST_BLESS=1 cargo test -p fluxcast-proto
--test vectors`.

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

## Control payloads (FEC, NACK, ACK)

These payloads ride inside the correspondingly typed FCDP datagrams. All
integers are big-endian. Canonical byte vectors are in
[`control-vectors.json`](control-vectors.json) and asserted by the Rust core.

- **FEC** (`FEC` datagram): `symbol_len: u16`, `fragment_count: u16`,
  `original_len: u32`, then `symbol_len` parity bytes. Source symbols are
  zero-padded to `symbol_len` before XOR, so one parity datagram repairs any
  single lost fragment of the frame. Because parity is as large as the biggest
  symbol, FEC-protected media fragments use a reduced symbol size so the parity
  datagram still fits the 1200-byte budget.
- **NACK** (`NACK` datagram): `count: u16`, then `count` big-endian `u32`
  sequence numbers being requested. Only audio and key-video sequences are
  eligible, and only while still within their deadline.
- **ACK** (`ACK` datagram): a fixed 16-byte receiver report — `sent: u32`,
  `received: u32`, `late: u32`, `rtt_micros: u32` — feeding the AIMD bitrate
  controller.

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
