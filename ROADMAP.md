# Roadmap

## M0 — specification and local interoperability (current)

- [x] Rust workspace and packet parser foundation
- [x] Packet round-trip and malformed-packet tests
- [ ] RFC-quality FCDP v0.1 specification and test vectors
- [ ] Localhost UDP sender/receiver and loss/reordering simulator

## Next milestones

- M1: encrypted, authenticated sessions; replay protection; sender/receiver;
  deadline handling; XOR FEC; NACK; and congestion control.
- M2: ICE/STUN/TURN connectivity, no-transcode authenticated fan-out Relay,
  authorization, metrics, and cross-network tests.
- M3: stable C ABI, Python/Node/Swift/Kotlin/Go SDKs, browser support, and
  interoperation tests across supported platforms.
- M4: adaptive bitrate and FEC, 4K pass-through, reproducible benchmarks,
  external security review, and release readiness.

No milestone makes a production-security claim without an external review.
