# Roadmap

## Product priority

FluxCast's first product objective is **ultra-low-latency media delivery that
remains usable on unstable mobile networks**. Work is ordered by its impact on
that objective: connection continuity across path changes, timely delivery
under loss and jitter, and evidence from repeatable impaired-network tests.
Features that do not improve these outcomes are secondary until the objective
has published evidence.

The release evidence must report, with reproducible test profiles:

- connection success and continuity during a network-path change;
- interruption time from detected path failure to resumed media;
- end-to-end latency distribution, not only an average;
- delivered audio continuity and video-frame delivery under loss, jitter, and
  bandwidth limits.

## M0 — specification and local interoperability (complete)

- [x] Rust workspace and packet parser foundation
- [x] Packet round-trip and malformed-packet tests
- [x] FCDP v0.1 specification and cross-language test vectors
      (`spec/test-vectors.json`, verified for Rust/Python/Node.js by
      `scripts/verify_vectors.sh`)
- [x] Localhost UDP sender/receiver and deterministic loss/reordering/expiry
      simulator (`fluxcast-core::simulation`, `fluxcast-cli simulate`)

## M1 — single-viewer transport (in progress)

Transport semantics are implemented and unit-tested; the remaining gate is
measured performance on real hardware and networks.

- [x] Encrypted, authenticated sessions with replay protection
      (`fluxcast-security`)
- [x] Deadline-aware sender/receiver pipeline: fragmentation, per-frame XOR FEC,
      audio/keyframe retransmission cache, deadline dropping, and NACK feedback
      (`fluxcast-core::pipeline`, `MediaSender`/`MediaReceiver`,
      `fluxcast-cli pipeline-demo`)
- [x] FEC/NACK/ACK control payload wire codecs
- [x] Conservative AIMD bitrate controller with keyframe-request feedback
- [ ] 1080p60 H.264 + Opus capture/encoder integration in an example app
- [ ] Measured LAN glass-to-glass ≤150 ms and continuous playback at 1% loss
- [ ] Reproducible mobile-network impairment profile: loss, jitter, bandwidth
      change, and path interruption, with published continuity and latency
      results

## M2 — connectivity and relay (in progress)

- [x] STUN server-reflexive discovery (`discover_server_reflexive_candidate`)
- [x] TURN relay allocation with long-term auth, permissions, channel binding,
      and `Data`-indication relaying (`fluxcast-core::turn`), validated against
      a remote TURN server
- [x] Authenticated ICE connectivity checks and `USE-CANDIDATE` nomination
      (`fluxcast-core::ice`), with candidate-pair ordering (`ordered_ice_pairs`)
- [x] No-transcode authenticated fan-out relay leases/metrics
      (`RelaySubscriptions`)
- [ ] Complete ICE connection management from gathered candidates and an
      application control plane for relay fallback. Ordered authenticated retry,
      credential restart, and tie-breaker role-conflict handling are implemented;
      `SecurePathEndpoint` switches its active path after an encrypted probe.
- [ ] Cross-network Publisher→Relay→Subscriber performance measurements
- [ ] Measured network-path handoff: encrypted path validation, resumed media,
      interruption time, and no unauthenticated-path promotion

## Next milestones
- M3: stable C ABI, Python/Node/Swift/Kotlin/Go SDKs, browser support
  (WebSocket→UDP gateway working and tested via `scripts/test_gateway.sh`), and
  interoperation tests across
  supported platforms.
- M4: adaptive bitrate and FEC, 4K pass-through, reproducible benchmarks,
  external security review, and release readiness.

No milestone makes a production-security claim without an external review.
