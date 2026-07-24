# Getting started

This guide takes a new checkout from zero to a verified local UDP exchange.
It needs no account, remote host, database, media device, or special network
configuration.

## Prerequisites

- Stable [Rust](https://rustup.rs/)
- Git
- Optional: Node.js 22+ and Python 3 for the cross-language checks
- Optional: a local media converter for `scripts/test_media_roundtrip.sh`

## Install and verify

```sh
git clone https://github.com/jota92/FluxCast.git
cd FluxCast
cargo test --workspace
cargo run -p fluxcast-cli -- demo
```

The final command uses loopback UDP and prints a send/receive result. It does
not contact the Internet.

## Send one packet between two terminals

Start a receiver in the first terminal:

```sh
cargo run -p fluxcast-cli -- receive 127.0.0.1:9000
```

Then send from the second terminal:

```sh
cargo run -p fluxcast-cli -- send 127.0.0.1:9000 "hello FluxCast"
```

If the receiver does not print the message, make sure another application is
not using UDP port 9000 and that both commands run on the same machine.

## Verify the repository before integrating

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bash scripts/verify_vectors.sh
bash scripts/test_gateway.sh
```

`verify_vectors.sh` needs Node.js and Python. `test_gateway.sh` needs Node.js
and only opens loopback ports for the duration of the test.

## What to do next

- Use a different language or application: [Integration guide](INTEGRATION.md)
- Send camera and microphone data: [Demo guide](../demo/README.md)
- Run the browser path: [Gateway guide](../gateway/README.md)
- Read the packet format: [FCDP v0.1](../spec/fcdp-v0.1.md)

FluxCast is pre-alpha. Keep tests local or in an environment you control; do
not use it for confidential media or a production workload.
