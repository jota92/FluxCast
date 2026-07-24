# Operations guide

This guide is for controlled experiments. It does not turn FluxCast into a
production service or replace an application security review.

## Roles and boundaries

| Component | Responsibility | Do not use it for |
| --- | --- | --- |
| Publisher | Creates media access units and holds application policy | Unauthenticated public publishing |
| Relay | Forwards already protected packets to authorized viewers | Decrypting or transcoding user media |
| Gateway | Carries protected browser packets between WebSocket and UDP | Identity, authorization, or TLS termination |
| Receiver | Reassembles, verifies, and renders data | Trusting malformed or unauthenticated input |

## Local gateway experiment

Set a random gateway token and a single UDP destination. The process binds to
loopback by design:

```sh
FLUXCAST_GATEWAY_TOKEN='replace-with-a-long-random-value' \
FLUXCAST_UDP_PEER='127.0.0.1:19100' \
node gateway/fluxcast-gateway.mjs
```

Use the gateway only behind a TLS terminator that enforces your application's
origin allow-list, authentication, token expiration, rate limits, and request
size limits. Never place the token in a public URL, source file, or log.

## Network experiments

- Open only the exact UDP ports required by the experiment.
- Restrict inbound sources to the known publisher or receiver addresses.
- Give remote hosts temporary credentials and stop them after the test.
- Record packet loss, late drops, retransmissions, and relay subscriber counts.
- Record `fluxcast_relay_send_failures_total`; after three consecutive local
  send failures, the relay removes only that subscriber and continues serving
  the remaining viewers.
- Remove test credentials and firewall rules when the experiment ends.

## Before sharing a deployment

Run the local checks in [Getting started](GETTING_STARTED.md), document the
revision and configuration you used, and review [SECURITY.md](../SECURITY.md).
The release blockers in that policy still apply.
