# FluxCast

> A Rust implementation of deadline-aware UDP primitives for low-latency media delivery.

FluxCast prioritizes the media that can still improve what a viewer sees or hears: audio and keyframes first; expired video is discarded instead of increasing latency. It is a new implementation and is not wire-compatible with WebRTC, SRT, RTP, QUIC, or MoQ.

## What works today

**Pre-alpha / M1 foundation.** The repository provides a codec-agnostic access-unit path that fragments data into 1200-byte-budget FCDP/UDP datagrams, validates headers, prioritizes audio/keyframes, drops expired queued packets, reassembles out-of-order fragments, performs one-loss XOR recovery, retains eligible audio/keyframe packets for retransmission, and exposes a conservative AIMD bitrate controller.

The current implementation also includes a signed forward-secret session handshake (Ed25519 + ephemeral X25519 + HKDF), ChaCha20-Poly1305 packet protection with replay rejection, RFC 5389 STUN server-reflexive candidate discovery, multi-subscriber relay leases/metrics, and CLI H.264 Annex-B / Ogg Opus send-and-recover paths. These capabilities have local and public-STUN runtime checks, but the project is still pre-alpha: ICE nomination/TURN allocation, connection migration, browser interoperability, stable SDKs, production relay control-plane persistence, long-duration impairment testing, and an independent security review are unfinished. Do not use it for production or confidential media.

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
- `fluxcast-core`: access-unit fragmentation, expiry scheduling, FEC, reassembly, retransmission cache, bitrate control, and a blocking UDP endpoint.
- `fluxcast-security`: signed forward-secret session handshake, ChaCha20-Poly1305 AEAD, and replay protection. Peer identity pinning/authorization remains an application responsibility.
- `fluxcast-cli`: local diagnostics and an end-to-end UDP demonstration.

The intended product direction is native Publisher/Relay/Subscriber traffic over FCDP/UDP, H.264 access units and Opus packets as the first media targets, plus a separate browser gateway. A future Relay will never decode or transcode media.

See [the protocol draft](spec/fcdp-v0.1.md), [the validation record](VALIDATION.md), [the roadmap](ROADMAP.md), and [contributing guidance](CONTRIBUTING.md).

## License

Apache-2.0. See [LICENSE](LICENSE).
