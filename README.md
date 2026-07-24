# FluxCast

> A Rust implementation of deadline-aware UDP primitives for low-latency media delivery.

FluxCast prioritizes the media that can still improve what a viewer sees or hears: audio and keyframes first; expired video is discarded instead of increasing latency. It is a new implementation and is not wire-compatible with WebRTC, SRT, RTP, QUIC, or MoQ.

## What works today

**Pre-alpha / M1 foundation.** The repository provides a codec-agnostic access-unit path that fragments data into 1200-byte-budget FCDP/UDP datagrams, validates headers, prioritizes audio/keyframes, drops expired queued packets, reassembles out-of-order fragments, performs one-loss XOR recovery, retains eligible audio/keyframe packets for retransmission, and exposes a conservative AIMD bitrate controller.

The current implementation also includes a cohesive deadline-aware media pipeline (`MediaSender`/`MediaReceiver`) that fragments access units, adds per-frame XOR FEC, retains audio/keyframes for retransmission, drops expired frames, drives NACK feedback, and adapts the send bitrate from authenticated receiver reports (AIMD congestion control with recovery keyframes) — demonstrated end to end by `fluxcast-cli pipeline-demo`.

For connectivity it provides RFC 5389 STUN server-reflexive discovery, a TURN client (RFC 5766/8489) for relayed NAT traversal — validated end to end against coturn on Azure — and an authenticated ICE connectivity-check agent (RFC 8445) with `USE-CANDIDATE` nomination. Security is a signed forward-secret handshake (Ed25519 + ephemeral X25519 + HKDF) with ChaCha20-Poly1305 packet protection and replay rejection. It also ships a WebSocket→UDP browser gateway (validated live and by `scripts/test_gateway.sh`), multi-subscriber relay leases, a Prometheus/OpenMetrics diagnostics module, and CLI H.264 Annex-B / Ogg Opus send-and-recover paths.

A deterministic loss/reordering/expiry simulator (`fluxcast-cli simulate`) demonstrates FEC recovery without a live network; `spec/test-vectors.json` and `spec/control-vectors.json` pin canonical FCDP framing and control payloads (checked across the Rust, Python, and Node.js SDKs); and CI builds and tests on Linux, macOS, and Windows. The project is still pre-alpha: a full ICE state machine, connection migration, WebTransport, stable SDK APIs, production relay control-plane persistence, measured glass-to-glass performance, and an independent security review are unfinished. Do not use it for production or confidential media.

## Quick start

```sh
cargo run -p fluxcast-cli
cargo test --workspace
cargo run -p fluxcast-cli -- demo
```

To test across two local terminals:

```sh
cargo run -p fluxcast-cli -- receive 127.0.0.1:9000
cargo run -p fluxcast-cli -- send 127.0.0.1:9000 "hello FluxCast"
```

## Direction

- `fluxcast-proto`: versioned FCDP packet framing and validation.
- `fluxcast-core`: access-unit fragmentation, expiry scheduling, FEC, reassembly, retransmission cache, congestion control, the `MediaSender`/`MediaReceiver` pipeline, STUN/TURN/ICE connectivity (`stun`/`turn`/`ice`), Prometheus metrics, an impairment simulator, and a blocking UDP endpoint.
- `fluxcast-security`: signed forward-secret session handshake, ChaCha20-Poly1305 AEAD, and replay protection. Peer identity pinning/authorization remains an application responsibility.
- `fluxcast-cli`: local diagnostics, STUN/TURN checks, an end-to-end UDP demonstration, and the impairment/pipeline demos.
- `gateway/`: a WebSocket→UDP browser gateway that never decrypts media.

The intended product direction is native Publisher/Relay/Subscriber traffic over FCDP/UDP, H.264 access units and Opus packets as the first media targets, plus a separate browser gateway. A future Relay will never decode or transcode media.

See [the protocol draft](spec/fcdp-v0.1.md), [the validation record](VALIDATION.md), [the roadmap](ROADMAP.md), and [contributing guidance](CONTRIBUTING.md).

## License

Apache-2.0. See [LICENSE](LICENSE).
