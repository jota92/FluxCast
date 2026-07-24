# Security policy

FluxCast is pre-alpha and has not received a security review. Do not use it to protect confidential media.

The repository includes independently tested X25519 key agreement,
ChaCha20-Poly1305 authenticated encryption, and replay protection. They are
not yet wired into a versioned network handshake or Relay authorization flow,
so the diagnostic CLI is not safe for untrusted networks.

Please report potential vulnerabilities privately to the repository maintainers using GitHub's private vulnerability-reporting feature once enabled. Until that is available, do not disclose exploit details in a public issue; contact the maintainer through the address listed in the repository profile.

Supported versions: none yet.
