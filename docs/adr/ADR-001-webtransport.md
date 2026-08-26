# ADR-001: WebTransport is the transport substrate

## Status

Accepted.

## Decision

FlexCast media semantics are implemented above HTTPS/TLS/HTTP/3/QUIC through
WebTransport. Visual atoms travel as unreliable datagrams; a session uses one
bidirectional reliable stream for control messages.

## Consequences

Datagram loss is expected and is handled by later state repair rather than by
waiting for historical media. The project does not implement transport crypto
or QUIC itself.
