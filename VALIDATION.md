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

## External Relay check

On 2026-07-24, a temporary FluxCast Relay and local receiver ran on the
project's persistent Azure test VM. A local publisher sent 13 H.264 NAL units
over the public Internet to UDP port 19100; the Azure Relay forwarded them to
the local subscriber. Input and recovered output had the same SHA-256:

`7475f8d06d1044045dd973e48ceee8229093485913c4e0a5c23f7dd1f9b5de68`

The VM was deallocated immediately after the check. The check proves a
specific public-Internet forwarding path, not NAT interoperability, sustained
performance, or security certification.

## TURN relay check

On 2026-07-24, the same Azure VM ran coturn with long-term authentication,
UDP/TCP listener port 3478, and relay ports limited to UDP 49152–49200. The
`turnutils_uclient` authenticated relay check completed with four messages sent
and received, zero loss, and a 0.25 ms average in-VM round-trip time. The VM
was deallocated immediately afterwards. Credentials are deployment secrets and
are deliberately not stored in this repository.

This proves coturn allocation/relay functionality, not FluxCast's unfinished
ICE nomination or automatic TURN fallback integration.

## Re-running

Run local checks with:

```sh
cargo test --workspace
cargo run -p fluxcast-cli -- secure-demo
cargo run -p fluxcast-cli -- simulate 0.02 300 1
cargo run -p fluxcast-cli -- stun stun.l.google.com:19302
bash scripts/verify_vectors.sh
bash scripts/test_media_roundtrip.sh
```

External validation requires a separately authorized VM, a narrowly scoped UDP
firewall rule, and a stop/deallocate action after testing.
