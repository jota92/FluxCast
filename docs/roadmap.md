# FlexCast implementation roadmap

The order below is mandatory: Transport → Capture → FCST → Visual State →
SURFACE → Renderer → Metrics → Loss → Repair → Freshness → Optimization.

## Initial sprint

- [x] FC-001 New monorepo foundation
- [x] FC-002 Rust WebTransport Edge acceptance path
- [x] FC-003 Browser WebTransport Sender path
- [x] FC-004 Session/invite API foundation
- [x] FC-005 Strict 1080p30 capture preflight and preview
- [x] FC-006 WebGPU capability gate and reference kernel contract
- [x] FC-007 TypeScript FCST binary encoder
- [x] FC-008 Rust FCST binary decoder and input limits
- [x] FC-009 Datagram transfer implementation
- [x] FC-010 60×45 Region analysis reference path
- [x] FC-011 Edge Visual State Store
- [x] FC-012 Independent FHD30 renderer clock
- [x] FC-013 Absolute SURFACE Atom
- [x] FC-014 Changed Region selection reference path
- [x] FC-015 Sender UI metrics foundation
- [x] FC-016 Edge metrics/state counters foundation
- [ ] FC-017 iPhone different-network measurement
- [ ] FC-018 Android different-network measurement

## Next gates

- [ ] Visual State Digest and authenticated short-lived publisher session
- [ ] Packet loss network simulator and atom assembly test suite
- [ ] Replacement REPAIR, REFRESH, and rotating truth refresh
- [ ] Freshness Debt scheduling and rate planner comparison against FIFO
- [ ] 48 kHz mono PCM audio, AudioWorklet, and A/V session clock
- [ ] Studio camera-source output interface

No unchecked item may be represented as complete. A change away from the FCST
visual-state approach requires an ADR with problem, experiment, measurements,
cause, alternatives, and requirement impact.
