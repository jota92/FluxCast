# External security review handoff

This is the handoff checklist for an independent reviewer. It records scope
and evidence, not an assertion that an audit has occurred.

## Review scope

1. FCDP parsing and packet-size/fragment validation.
2. Ed25519/X25519/HKDF handshake transcript binding and peer authorization API.
3. ChaCha20-Poly1305 nonce uniqueness, associated data, replay window, and key
   lifecycle.
4. Relay and browser Gateway exposure, authorization, resource limits, and
   denial-of-service behavior.
5. STUN/ICE/TURN and migration code when implemented.

## Evidence included in this repository

- `spec/fcdp-v0.1.md`: packet and handshake profile.
- `crates/fluxcast-security`: implementation and tamper/replay/peer-mismatch
  tests.
- `crates/fluxcast-proto`: strict decoder plus deterministic malformed-input
  regression coverage.
- `VALIDATION.md`: local and external data-plane evidence.
- `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo test --workspace`: required baseline checks.

## Required reviewer deliverables

- Versioned report listing threat model, findings, severity, reproduction, and
  remediation verification.
- Dependency/SBOM and licensing review.
- Fuzzing and resource-exhaustion results for packet and control-plane inputs.
- Explicit decision on protocol suitability before any production claim.
