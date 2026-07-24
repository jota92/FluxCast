# Project governance and public scope

FluxCast is an open-source, pre-alpha protocol implementation. This document
states what the public repository grants and what it does not promise.

## Permissions

FluxCast is released under [Apache-2.0](LICENSE). Subject to that license, you
may use, copy, modify, distribute, and use the software commercially. The
license includes a patent grant from contributors and its stated patent
termination condition. Keep the required license and attribution notices when
redistributing it.

The license does not grant rights to a FluxCast trademark, endorsement, hosted
service, security certification, compatibility guarantee, or support contract.
This repository currently has no official hosted relay, TURN service, or cloud
account for public use.

## Compatibility and support boundaries

- FCDP v0.1 is a draft protocol profile. Its packet format and public APIs may
  change before a stable release.
- Public APIs and packet formats can change before a stable release. Pin a
  commit or release and run the published vector tests when integrating.
- Security-sensitive and production use are out of scope until an independent
  review and the release blockers in [SECURITY.md](SECURITY.md) are resolved.
- Contributors retain copyright in their contributions and grant their changes
  under Apache-2.0 by submitting a pull request, unless agreed otherwise in
  writing before submission.

## Decisions and contributions

The maintainer reviews changes through public GitHub issues and pull requests.
Open an issue before large protocol, cryptographic, or interoperability changes.
Protocol changes must update the draft and test vectors in the same pull
request. Security reports follow [SECURITY.md](SECURITY.md), not public issues.

## Repository hygiene

The public repository contains source code, reproducible tests, documentation,
and intentionally published protocol vectors. It must not contain credentials,
private endpoints, AI prompts, private requirement documents, user data, or
generated build and media artifacts. Report an accidental disclosure privately
under the security policy.
