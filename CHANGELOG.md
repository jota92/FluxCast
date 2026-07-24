# Changelog

All notable changes will be recorded here.

## 0.1.0 — unreleased

- Added FCDP v0.1 packet framing and validation.
- Added access-unit fragmentation/reassembly, deadline scheduling, XOR FEC, retransmission cache, bitrate controller, and UDP diagnostics.
- Added a deterministic loss/reordering/expiry simulator (`fluxcast-core::simulation`) and a `fluxcast-cli simulate` command that reports frame delivery and FEC recovery rates.
- Added canonical cross-language FCDP v0.1 test vectors (`spec/test-vectors.json`) verified across the Rust, Python, and Node.js SDKs by `scripts/verify_vectors.sh`.
- Added OSS governance, security, contribution, CI, and issue-reporting foundations.
