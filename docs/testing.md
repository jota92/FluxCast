# Testing and measurement

## Repeatable local checks

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm build
pnpm test
```

The FCST golden vector is encoded by TypeScript and decoded by Rust. It pins
the 40-byte header and a complete 73-byte `SURFACE` payload.

## Required real-device validation

The initial implementation is not a claim that the mobile acceptance criteria
are met. Run and record each test before marking the corresponding gate done:

| Gate | Device/network | Evidence |
| --- | --- | --- |
| FC-017 | iPhone Safari on cellular, Edge on a different network | 100,000 datagrams, capture settings, loss, RTT, mean/p95 Region Age |
| FC-018 | Android Chrome on cellular, Edge on a different network | Same metrics and connection log |
| Network switch | Wi-Fi → cellular during one session | session epoch, interruption, time to convergence |
| 30 minute run | iPhone and Android independently | processing p50/p95, capture fps, memory and battery observations |

Raw frames, visual atom payloads, and PCM samples must not be saved with these
results. Record numerical telemetry only.

## Initial metrics contract

Sender: capture FPS, processing FPS, processing p50/p95, generated/sent atoms,
dropped-expired atoms, queue age, target/sent bitrate, RTT, jitter, and loss.

Edge: received datagrams, sequence gaps, invalid/expired/applied atoms, mean and
p95 Region Age, render FPS, render duration, audio loss, and state convergence
time.
