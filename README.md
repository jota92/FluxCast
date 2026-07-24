# FluxCast

> A Rust implementation of deadline-aware UDP primitives for low-latency media delivery.

FluxCast prioritizes the media that can still improve what a viewer sees or hears: audio and keyframes first; expired video is discarded instead of increasing latency. It is a new implementation and is not wire-compatible with WebRTC, SRT, RTP, QUIC, or MoQ.

## What works today

**Pre-alpha / M1 foundation.** The repository provides a codec-agnostic access-unit path that fragments data into 1200-byte-budget FCDP/UDP datagrams, validates headers, prioritizes audio/keyframes, drops expired queued packets, reassembles out-of-order fragments, performs one-loss XOR recovery, retains eligible audio/keyframe packets for retransmission, and exposes a conservative AIMD bitrate controller.

It does **not** yet provide a versioned network handshake, NAT traversal, an H.264/Opus capture pipeline, browser support, or stable language bindings. It must not be used for confidential media or production traffic.

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
- `fluxcast-security`: X25519 key agreement, ChaCha20-Poly1305 AEAD, and a replay window. Peer identities must be pinned out of band.
- `fluxcast-cli`: local diagnostics and an end-to-end UDP demonstration.

The intended product direction is native Publisher/Relay/Subscriber traffic over FCDP/UDP, H.264 access units and Opus packets as the first media targets, plus a separate browser gateway. A future Relay will never decode or transcode media.

See [the protocol draft](spec/fcdp-v0.1.md), [the roadmap](ROADMAP.md), and [contributing guidance](CONTRIBUTING.md).

## License

Apache-2.0. See [LICENSE](LICENSE).
