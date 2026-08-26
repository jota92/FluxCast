# Architecture

```text
Sender website (HTTPS)
  camera + microphone
  └─ capture worker ─ region analysis ─ FCST SURFACE atoms
        └─ WebTransport unreliable datagrams ─ FlexCast Edge
              └─ validated atom assembly ─ Visual State ─ 1080p30 renderer
```

One bidirectional reliable WebTransport stream carries session control. Media
uses datagrams only. A region update is applied only after the complete atom is
available and its base state matches.

The current edge implementation renders an RGBA FHD framebuffer on a fixed
33.333 ms clock. The next milestones add visual-state digest, repair, and
freshness scheduling without changing this boundary.
