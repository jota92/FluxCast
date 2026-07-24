# Changelog

All notable changes will be recorded here.

## 0.1.0 — unreleased

- Added FCDP v0.1 packet framing and validation.
- Added access-unit fragmentation/reassembly, deadline scheduling, XOR FEC, retransmission cache, bitrate controller, and UDP diagnostics.
- Added a deterministic loss/reordering/expiry simulator (`fluxcast-core::simulation`) and a `fluxcast-cli simulate` command that reports frame delivery and FEC recovery rates.
- Added canonical cross-language FCDP v0.1 test vectors (`spec/test-vectors.json`) verified across the Rust, Python, and Node.js SDKs by `scripts/verify_vectors.sh`.
- Added the M1 media pipeline (`fluxcast-core::pipeline`): `MediaSender`/`MediaReceiver` that wire fragmentation, per-frame XOR FEC, audio/keyframe retransmission, deadline dropping, and NACK feedback into one flow, with FEC/NACK/ACK wire codecs and an end-to-end `fluxcast-cli pipeline-demo`.
- Added `fragment_access_unit_sized` so FEC-protected frames use a symbol size that keeps parity within the datagram budget.
- Added a TURN (RFC 5766/8489) client (`fluxcast-core::turn`) with long-term authentication (MD5 + HMAC-SHA1 `MESSAGE-INTEGRITY`), Allocate/CreatePermission/ChannelBind/Refresh, and `ChannelData`/`Data`-indication relaying, plus `fluxcast-cli turn`/`turn-recv`. Validated end-to-end against coturn on Azure.
- Added a shared STUN codec (`fluxcast-core::stun`) and an authenticated ICE connectivity-check agent (`fluxcast-core::ice`, RFC 8445 short-term credentials): STUN Binding checks with `MESSAGE-INTEGRITY`, `USE-CANDIDATE` nomination by the controlling agent, and rejection of forged checks.
- Fixed the browser gateway serving a 404 for `/?token=...` (it now matches on the URL path), and added `scripts/test_gateway.sh`, a reproducible WebSocket→gateway→UDP→receiver round-trip check wired into CI.
- Added OSS governance, security, contribution, CI, and issue-reporting foundations.
