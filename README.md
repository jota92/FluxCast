# FluxCast

> A Rust media transport focused on keeping ultra-low-latency streams alive on unstable mobile networks.

[日本語](docs/ja/README.md) | [简体中文](docs/zh-CN/README.md) | **English**

**Start here:** FluxCast is an experimental, native UDP media transport for
applications that need to make latency-aware delivery decisions. It is
pre-alpha and is not ready for production or confidential media. If that fits
your experiment, the following two commands
give you a safe local first run:

```sh
cargo test --workspace
cargo run -p fluxcast-cli -- demo
```

FluxCast's product priority is clear: keep an ultra-low-latency stream usable
through loss, jitter, NAT changes, and mobile-network handoffs. It prioritizes
the media that can still improve what a viewer sees or hears: audio and
keyframes first; expired video is discarded instead of increasing latency. Its
FCDP packet format is currently a draft and may change before a stable release.

## What works today

**Pre-alpha / M1 foundation.** The repository provides a codec-agnostic access-unit path that fragments data into 1200-byte-budget FCDP/UDP datagrams, validates headers, prioritizes audio/keyframes, drops expired queued packets, reassembles out-of-order fragments, performs one-loss XOR recovery, retains eligible audio/keyframe packets for retransmission, and exposes a conservative AIMD bitrate controller.

The current implementation also includes a cohesive deadline-aware media pipeline (`MediaSender`/`MediaReceiver`) that fragments access units, adds per-frame XOR FEC, retains audio/keyframes for retransmission, drops expired frames, drives NACK feedback, and adapts the send bitrate from authenticated receiver reports (AIMD congestion control with recovery keyframes) — demonstrated end to end by `fluxcast-cli pipeline-demo`.

For connectivity it provides RFC 5389 STUN server-reflexive discovery, a TURN client (RFC 5766/8489) for relayed NAT traversal, and an authenticated ICE connectivity-check agent (RFC 8445) with `USE-CANDIDATE` nomination, ordered-candidate retry, and credential restart. `SecurePathEndpoint` promotes a new active path only after an encrypted probe succeeds. Security is a signed forward-secret handshake (Ed25519 + ephemeral X25519 + HKDF) with ChaCha20-Poly1305 packet protection and replay rejection. It also ships a WebSocket→UDP browser gateway (validated live and by `scripts/test_gateway.sh`), multi-subscriber relay leases that isolate repeated failed sends to one viewer, an OpenMetrics diagnostics module, and CLI H.264 Annex-B / Ogg Opus send-and-recover paths.

A deterministic loss/reordering/expiry simulator (`fluxcast-cli simulate`) demonstrates FEC recovery without a live network; `spec/test-vectors.json` and `spec/control-vectors.json` pin canonical FCDP framing and control payloads, with reproducible Rust, Python, Node.js, Go, and Swift checks. Kotlin uses the same checked-in vector test when its local toolchain is explicitly enabled. The project is still pre-alpha: a full ICE state machine, connection migration, stable SDK APIs, production relay control-plane persistence, measured glass-to-glass performance, and an independent security review are unfinished. Do not use it for production or confidential media.

## Quick start

### 1. Install Rust

Install the stable Rust toolchain with [rustup](https://rustup.rs/), then clone
this repository. No database, cloud account, or media device is required for
the local demo.

```sh
git clone https://github.com/jota92/FluxCast.git
cd fluxcast
cargo test --workspace
cargo run -p fluxcast-cli -- demo
```

To test across two local terminals:

```sh
cargo run -p fluxcast-cli -- receive 127.0.0.1:9000
cargo run -p fluxcast-cli -- send 127.0.0.1:9000 "hello FluxCast"
```

### 2. Choose an integration path

- **Rust application:** use the workspace crates directly while APIs are
  stabilizing: `fluxcast-proto`, `fluxcast-core`, and `fluxcast-security`.
- **Protocol integration:** start from the pinned byte examples in
  [`spec/test-vectors.json`](spec/test-vectors.json), then use the minimal
  SDKs in [`sdk/`](sdk/). Python and Node vectors are verified by the included
  scripts; Go,
  Swift, and Kotlin bindings are early-stage reference implementations.
- **Browser experiment:** run the documented
  [WebSocket-to-UDP gateway](gateway/README.md) behind TLS and your own
  authentication/origin policy. It is not a public Internet service by itself.
- **Camera demonstration:** see [`demo/`](demo/) for camera/microphone to HLS
  playback. It is a demonstration deployment, not a production template.

### 3. Verify before changing the protocol

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bash scripts/verify_vectors.sh
bash scripts/test_gateway.sh
```

## Documentation

| Task | Guide |
| --- | --- |
| First local run | [Getting started](docs/GETTING_STARTED.md) |
| Add FluxCast to an application | [Integration guide](docs/INTEGRATION.md) |
| Run a gateway or controlled network experiment | [Operations guide](docs/OPERATIONS.md) |
| Browse all guides and the repository map | [Documentation index](docs/README.md) |

Japanese and Simplified Chinese quick-start guides are available in
[`docs/ja/`](docs/ja/GETTING_STARTED.md) and
[`docs/zh-CN/`](docs/zh-CN/GETTING_STARTED.md).

## Product focus and proof

FluxCast is being built first for **ultra-low-latency delivery that stays
connected on unstable mobile networks**. This is a measurable release goal,
not a marketing claim. A release in this direction must publish repeatable
results for connection continuity, handoff interruption time, end-to-end
latency, and delivered media quality under controlled loss, jitter, bandwidth,
and path-change conditions. See [the roadmap](ROADMAP.md) and [the validation
record](VALIDATION.md) for the current gates and evidence.

## Components

- `fluxcast-proto`: versioned FCDP packet framing and validation.
- `fluxcast-core`: access-unit fragmentation, expiry scheduling, FEC, reassembly, retransmission cache, congestion control, the `MediaSender`/`MediaReceiver` pipeline, STUN/TURN/ICE connectivity (`stun`/`turn`/`ice`), Prometheus metrics, an impairment simulator, and a blocking UDP endpoint.
- `fluxcast-security`: signed forward-secret session handshake, ChaCha20-Poly1305 AEAD, and replay protection. Peer identity pinning/authorization remains an application responsibility.
- `fluxcast-cli`: local diagnostics, STUN/TURN checks, an end-to-end UDP demonstration, and the impairment/pipeline demos.
- `gateway/`: a WebSocket→UDP browser gateway that never decrypts media.

The intended product direction is native Publisher/Relay/Subscriber traffic over FCDP/UDP, H.264 access units and Opus packets as the first media targets, plus a separate browser gateway. A future Relay will never decode or transcode media.

See [the protocol draft](spec/fcdp-v0.1.md), [the validation record](VALIDATION.md), [the roadmap](ROADMAP.md), [security guidance](SECURITY.md), [project governance](GOVERNANCE.md), and [contributing guidance](CONTRIBUTING.md).

## License

Apache-2.0. You may use, modify, redistribute, and use FluxCast commercially,
subject to its notice, license, and patent-termination terms. The FluxCast name
and logos (if introduced) are not granted as trademarks. See [LICENSE](LICENSE)
and [GOVERNANCE.md](GOVERNANCE.md).
