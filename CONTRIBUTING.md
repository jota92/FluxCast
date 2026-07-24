# Contributing

Thank you for helping FluxCast. Please open an issue before large protocol or security changes so that interoperability implications can be discussed first.

## Development

Install the Rust toolchain specified in `rust-version`, then run:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Keep changes small, add tests for wire-format changes, and update `spec/` in the same pull request. Protocol compatibility changes require a version-negotiation story and test vectors.

## Security

Do not file security vulnerabilities in public issues. Follow [SECURITY.md](SECURITY.md).
