# Roadmap

## M0 — specification and local interoperability (current)

- [x] Rust workspace and packet parser foundation
- [x] Packet round-trip and malformed-packet tests
- [ ] RFC-quality FCDP v0.1 specification and test vectors
- [ ] Localhost UDP sender/receiver and loss/reordering simulator

## Next milestones

- M1: sender/receiver, deadline handling, XOR FEC and NACK
- M2: no-transcode fan-out Relay and metrics
- M3: C ABI, Python/Node SDKs and a WebTransport gateway
- M4: adaptive quality, 4K pass-through and reproducible benchmarks

No milestone makes a production-security claim without an external review.
