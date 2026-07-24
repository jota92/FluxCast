# FluxCast browser gateway

The gateway relays **already encrypted** FCDP packets between a browser
WebSocket and one configured UDP peer. It never receives identity keys or
decrypts media. Bind it behind TLS/reverse-proxy infrastructure in deployment.

```sh
FLUXCAST_GATEWAY_TOKEN='long-random-secret' \
FLUXCAST_UDP_PEER='127.0.0.1:19100' \
node gateway/fluxcast-gateway.mjs
```

It binds to loopback only. A reverse proxy must terminate TLS and perform
origin/rate-limit policy. Do not expose the process directly to the Internet.
