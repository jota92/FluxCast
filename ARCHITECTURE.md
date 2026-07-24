# Architecture

FluxCast separates packet syntax from media-delivery policy.

```text
encoded access unit
  → fragment_access_unit (FCDP MEDIA datagrams, ≤1200 bytes)
  → DeadlineQueue / RetransmitWindow
  → UdpEndpoint
  → validated FCDP datagram
  → Reassembler
  → encoded access unit
```

`fluxcast-proto` owns the versioned wire header. `fluxcast-core` is codec agnostic: H.264, AV1, and Opus integration belongs at the SDK edge, so the core never decodes or transcodes media.

## Delivery policy

- Audio and key video are priority 0 and are eligible for retransmission.
- Delta video is lower priority and is never retained for retransmission.
- A queued datagram whose deadline has passed is discarded before it can use bandwidth.
- The initial FEC scheme is XOR parity and can recover exactly one missing fragment in a block.
- The bitrate controller immediately reduces the target after meaningful loss or late delivery, and only increases after a stable interval.

## Security boundary

The current code validates framing only. CRC-16 detects accidental corruption; it is not authentication. A networked release requires a versioned handshake, replay protection, key rotation, rate limiting, and audited AEAD encryption. Those requirements are intentionally not simulated or claimed as complete here.
