# Security policy

FluxCast is pre-alpha and has **not** received an independent security review.
Do not use it to protect confidential media or expose its diagnostic services to
untrusted networks.

## Current cryptographic boundary

- Session setup signs ephemeral X25519 keys with Ed25519 identities and derives
  direction-specific ChaCha20-Poly1305 keys with HKDF-SHA256.
- FCDP header fields (other than CRC) are AEAD associated data; receivers reject
  unauthenticated, replayed, or session-mismatched packets.
- Applications must pin or otherwise authorize the Ed25519 identities. The
  library deliberately does not invent a trust service or account model.
- A Relay should forward ciphertext only. It must not hold end-to-end media
  keys. Gateway and Relay control planes are not a substitute for application
  authentication.
- STUN discovery, authenticated ICE connectivity checks with `USE-CANDIDATE`,
  and TURN Allocate/CreatePermission/ChannelBind/Refresh are implemented.
  They are building blocks, not a complete connection manager.

## Known release blockers

- No third-party cryptographic/protocol audit has been performed.
- The ICE layer has authenticated ordered retry and credential restart, while
  `SecurePathEndpoint` promotes paths only after encrypted probes. Candidate
  gathering and an application control plane for relay fallback remain
  unfinished.
- No automatic key rotation, certificate lifecycle, or persistent
  authorization service exists yet.
- The diagnostic CLI uses plaintext FCDP unless a caller constructs a secure
  session; it is not a secure publishing application.
- The browser Gateway requires a TLS reverse proxy and an application-owned
  origin, rate-limit, and token-lifecycle policy before exposure to users.

## Reporting

Please report potential vulnerabilities privately using GitHub's private
vulnerability-reporting feature once enabled. Until that is available, do not
publish exploit details in a public issue; contact the maintainer through the
address listed in the repository profile.

Supported versions: none yet.
