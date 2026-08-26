# FCST/1

FCST is the FlexCast State Transport binary protocol. All integer fields are
big-endian. Every received datagram is untrusted input.

## Datagram header (40 bytes)

| Offset | Size | Field |
| --- | ---: | --- |
| 0 | 2 | magic `FC 01` |
| 2 | 1 | major version (`1`) |
| 3 | 1 | atom type |
| 4 | 2 | flags |
| 6 | 2 | header length (`40`) |
| 8 | 4 | session epoch |
| 12 | 4 | atom sequence |
| 16 | 4 | frame tick |
| 20 | 2 | region id (0–2699) |
| 22 | 1 | fragment index |
| 23 | 1 | fragment count (1–8) |
| 24 | 4 | state id |
| 28 | 4 | base state id |
| 32 | 4 | capture monotonic time in ms |
| 36 | 2 | TTL in ms |
| 38 | 2 | payload length |

The initial `SURFACE` payload is absolute FlexColor control data:

```text
quantization: u8
luma:        48 × u8
chroma_a:    12 × i8
chroma_b:    12 × i8
```

An incomplete fragmented atom is never applied. The Edge bounds fragments to
eight and removes incomplete atom assemblies at `min(ttl, 100 ms)`.
