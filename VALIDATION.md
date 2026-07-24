# Validation record

This document records repeatable checks; it is not a claim that FluxCast is
production-ready.

## Local media round trip

On 2026-07-24, FFmpeg generated one-second H.264 Annex-B and Ogg Opus inputs.
They were sent through the FCDP CLI and recovered with `receive-file`.

| Stream | Input bytes | Recovered bytes | Result |
| --- | ---: | ---: | --- |
| H.264 Annex-B | 9,848 | 9,848 | FFmpeg decoded recovered stream |
| Ogg Opus | 10,062 | 10,062 | FFmpeg decoded recovered stream |

## Deterministic impairment simulation

`fluxcast-core::simulation` drives real fragmented access units through a seeded
loss/reordering/expiry channel and recovers them with the production XOR-parity
and deadline logic. Because it is seeded, the numbers are reproducible on any
platform without a live network. Example (`cargo run -p fluxcast-cli -- simulate
0.01 600 7`): at 1% datagram loss with reordering, every transmitted frame is
delivered (single-fragment losses are recovered by parity), matching the spec's
"no video stall at 1% loss" target. Raise the loss rate to observe parity
saturation and deadline drops. The `simulation` unit tests assert clean
delivery, reorder tolerance, single-loss recovery, deadline dropping, and
per-seed determinism.

## Cross-language wire test vectors

`spec/test-vectors.json` pins canonical FCDP v0.1 packets. `cargo test -p
fluxcast-proto --test vectors` proves the Rust reference reproduces the file
byte-for-byte and round-trips every vector; `scripts/verify_vectors.sh`
additionally confirms the Python and Node.js SDKs encode and decode the same
bytes. This check runs in CI.

## Browser gateway round trip

`scripts/test_gateway.sh` starts the WebSocket→UDP gateway and a native
receiver, then a WebSocket client (Node 22+ global `WebSocket`, the same API a
browser uses) sends the canonical `media_minimal` FCDP datagram. The receiver
reassembles it and prints `opus-or-h264`, confirming the browser path
(WebSocket → gateway → FCDP/UDP → native receiver) without the gateway ever
decrypting media. The in-app browser was additionally driven live through the
same page (`window.fluxcastSend`) with the same result.

## External Relay check

On 2026-07-24, a temporary FluxCast Relay and local receiver ran on the
project's persistent Azure test VM. A local publisher sent 13 H.264 NAL units
over the public Internet to UDP port 19100; the Azure Relay forwarded them to
the local subscriber. Input and recovered output had the same SHA-256:

`7475f8d06d1044045dd973e48ceee8229093485913c4e0a5c23f7dd1f9b5de68`

The VM was deallocated immediately after the check. The check proves a
specific public-Internet forwarding path, not NAT interoperability, sustained
performance, or security certification.

## TURN relay check (FluxCast client)

On 2026-07-24, the `fluxcast-lab` Azure VM (East Asia) ran coturn with
long-term authentication (`lt-cred-mech`, realm `fluxcast`), UDP listener 3478,
and relay ports UDP 49152–49200. FluxCast's own `TurnClient`
(`fluxcast-core::turn`) was driven from a home network behind NAT:

- `fluxcast-cli turn` completed the 401 challenge and an authenticated
  Allocate, `CreatePermission`, and `ChannelBind`, each carrying a valid
  `MESSAGE-INTEGRITY` (MD5 long-term key + HMAC-SHA1). It received relayed
  address `20.205.121.106:49189` and correctly reported its server-reflexive
  mapping `219.28.140.145:53871`.
- A full data-plane relay was then verified: `fluxcast-cli turn-recv`
  allocated relay `20.205.121.106:49160` and permitted the peer IP; a peer
  process on the VM sent to the relay; coturn forwarded the datagram across the
  public Internet as a STUN `Data` indication, which the client decoded to the
  original 19-byte payload from peer `20.205.121.106:43884`.

This exercises FluxCast's TURN allocation, authentication, permissions, channel
binding, and `Data`-indication handling against production coturn. Credentials
are ephemeral deployment secrets and are deliberately not stored in this
repository. Authenticated ICE nomination is implemented; complete ICE check
scheduling and automatic TURN fallback wiring remain in progress. The classic
`turnutils_uclient` check also passed previously.

## Re-running

Run local checks with:

```sh
cargo test --workspace
cargo run -p fluxcast-cli -- secure-demo
cargo run -p fluxcast-cli -- simulate 0.02 300 1
cargo run -p fluxcast-cli -- pipeline-demo 0.15 120
cargo run -p fluxcast-cli -- stun stun.l.google.com:19302
bash scripts/verify_vectors.sh
bash scripts/test_media_roundtrip.sh
```

External validation requires a separately authorized VM, a narrowly scoped UDP
firewall rule, and a stop/deallocate action after testing.
