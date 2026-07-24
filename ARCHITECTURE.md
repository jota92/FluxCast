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

CRC-16 detects accidental corruption; it is not authentication. The secure
transport path has a signed ephemeral handshake, AEAD encryption, and replay
rejection. Identity authorization, key rotation, rate limiting, and an
independent review remain release gates. The diagnostic `send`/`receive` CLI
commands intentionally use plaintext FCDP and are not a secure application.

## Release baseline

FluxCast will not describe itself as a WebRTC-class transport until it provides
the following as maintained, tested product capabilities: ICE/STUN/TURN NAT
traversal; authenticated encrypted sessions (the role served by DTLS-SRTP in
WebRTC); congestion control and adaptive bitrate; browser interoperability;
stable multi-platform SDKs; diagnostics; and reproducible interoperation tests.
These are release gates, not optional enhancements.
