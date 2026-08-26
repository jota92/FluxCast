# FlexCast

FlexCast is the media-ingress foundation for Cloud Studio: a phone opens an
HTTPS page, captures camera and microphone, and synchronizes a custom visual
state with a remote Edge over WebTransport datagrams.

It does **not** use WebRTC, RTP, RTMP, HLS, or a conventional video codec in
the initial visual path. The Edge continuously renders a 1920×1080 visual
state at 30 fps. Under pressure the sender reduces visual detail and the number
of updated regions; it does not silently lower the FHD30 contract.

## Current sprint

The initial path is implemented as a local, testable PoC:

```text
HTTPS browser → getUserMedia → 60×45 Region analysis → SURFACE atoms
              → FCST/WebTransport datagrams → Rust Edge
              → Visual State Store → 1920×1080 / 30 fps renderer
```

It includes FCST binary golden tests, server-side validation and fragment
assembly, state metrics, a 30 fps render clock, changed-region detection, and
a mobile preflight page. Audio PCM, visual digests, replacement repair, and
freshness scheduling are deliberately sequenced after this base path.

## Local development

Requirements: current Rust, Node.js 22+, pnpm, and a TLS certificate trusted by
the browser. WebTransport requires HTTPS and HTTP/3; an ordinary HTTP server is
not a substitute.

```sh
pnpm install
pnpm dev
cargo test --workspace
cargo run -p edge-gateway -- --cert ./certs/localhost.pem --key ./certs/localhost-key.pem
```

Open the sender page through HTTPS, create a session, and enter the Edge URL.
For exact configuration, see [local development](docs/development.md) and the
[protocol](docs/protocol/fcst-v1.md).

## QR camera demo

For a complete PC-to-phone flow, the included Studio demo creates a QR invitation
on the PC. Scanning it opens the browser sender on the phone; its rendered
Visual State appears back on the PC. The QR uses a LAN or public HTTPS origin,
not `localhost`, because a phone's `localhost` is the phone itself. Follow the
[QR camera demo guide](docs/demo.md).

## Repository map

- `apps/sender-web` — install-free smartphone sender and strict FHD30 preflight
- `apps/demo-studio` — PC QR invitation and Edge Visual State preview
- `services/edge-gateway` — Rust HTTP/3/WebTransport Edge
- `services/demo-server` — same-origin HTTPS host for the QR demo
- `services/session-api` — short-lived invite and publisher-token API
- `crates/` — FCST parser, state store, renderer, scheduler foundations
- `shaders/` — WebGPU analysis/reconstruction kernels
- `tests/` — protocol and visual-state golden tests
- `docs/adr` — design decisions and measured change proposals

## Status and safety

This repository is a PoC, not a production service. It has not yet completed
real iPhone/Android cross-network validation, independent security review, or
the subsequent repair/audio phases. It never persists raw video or atom
payloads in application logs.

Apache-2.0. See [LICENSE](LICENSE).
