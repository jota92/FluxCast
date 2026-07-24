# FluxCast documentation

This directory is the documentation entry point after the repository README.
Choose the shortest path that matches what you are trying to do.

| Goal | Start here |
| --- | --- |
| Run FluxCast locally | [Getting started](GETTING_STARTED.md) |
| Encode or decode FCDP from an application | [Integration guide](INTEGRATION.md) |
| Run a gateway or relay safely in an experiment | [Operations guide](OPERATIONS.md) |
| Understand packet fields and wire compatibility | [FCDP v0.1 draft](../spec/fcdp-v0.1.md) |
| Check what was actually tested | [Validation record](../VALIDATION.md) |

Localized entry points: [日本語](ja/README.md) and [简体中文](zh-CN/README.md).

## Repository map

```text
crates/    Rust implementation and command-line diagnostics
sdk/       Small framing libraries for other languages
examples/  Small programs that send one valid FCDP datagram
gateway/   Browser-to-UDP experimental gateway
demo/      Camera-to-browser demonstration
spec/      Versioned FCDP draft and canonical byte vectors
docs/      Setup, integration, and operational guidance
scripts/   Reproducible local verification commands
```

The public API and packet format are pre-release. Pin the revision you test,
run the vector checks after updating, and do not treat this repository as a
production service.
