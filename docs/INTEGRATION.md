# Integration guide

FluxCast is organized so that an application can choose the smallest layer it
needs. Start with FCDP framing; add the Rust media pipeline, secure session,
or browser gateway only when your application owns the surrounding policy.

## Choose a layer

| Need | Use | Status |
| --- | --- | --- |
| Encode/decode one FCDP datagram | `fluxcast-proto` or a `sdk/` module | Available; draft wire format |
| Fragment media, FEC, NACK, deadline handling | `fluxcast-core` | Available in Rust |
| Signed session and packet protection | `fluxcast-security` | Available in Rust |
| Browser-to-UDP experiment | `gateway/` | Requires application-owned TLS and access control |
| Relay and connectivity diagnostics | `fluxcast-cli` | Experimental diagnostic tools |

## Rust application

FluxCast crates are not published as a stable package. The safest current
integration is a checkout pinned by your application, then local path
dependencies:

```toml
# Cargo.toml
[dependencies]
fluxcast-proto = { path = "../FluxCast/crates/fluxcast-proto" }
fluxcast-core = { path = "../FluxCast/crates/fluxcast-core" }
fluxcast-security = { path = "../FluxCast/crates/fluxcast-security" }
```

Pin a commit in your own dependency management and rerun the vector tests when
you update. Use `fluxcast-proto` for framing, `fluxcast-core` for media
transport behavior, and `fluxcast-security` only after your application has an
explicit policy for authorizing peer identities.

For controlled connectivity, give `IceAgent::nominate_first_reachable` remote
candidates in your preferred order. It retries each candidate with authenticated
checks, nominates the first reachable path, and returns its measured RTT.
Call `IceAgent::restart` only after exchanging replacement credentials through
your application's authenticated signalling channel.

## Python and Node.js

The Python and Node modules are dependency-free framing references. They are
not installable packages yet; import them from a checked-out copy of this
repository.

```sh
# Python: make the local module visible, then run the sample
PYTHONPATH="$PWD/sdk/python" python3 examples/python/send_fcdp.py 127.0.0.1 9000 hello

# Node.js: run the standard-library-only sample
node examples/node/send-fcdp.mjs 127.0.0.1 9000 hello
```

For your own code, import `fluxcast.fcdp` in Python or
`sdk/node/fluxcast-fcdp.mjs` in Node.js. Both expose strict `encode` and
`decode` operations and reject invalid packet framing.

## Go, Swift, Kotlin, and C

- **Go:** use the local `sdk/go` module during evaluation. It exposes
  `fcdp.Encode` and `fcdp.Decode`.
- **Swift:** add `sdk/swift` as a local package in your project, then import
  `FluxCast` and use `FCDP.encode` / `FCDP.decode`.
- **Kotlin:** copy or include `sdk/kotlin/src/main/kotlin/FluxCastFcdp.kt` in
  the application module, then call `FluxCastFcdp.encode` / `decode`.
- **C:** `examples/c/send_fcdp.c` is a single-datagram POSIX sample:

  ```sh
  cc -O2 examples/c/send_fcdp.c -o /tmp/fluxcast-send
  /tmp/fluxcast-send 127.0.0.1 9000 hello
  ```

These modules share the canonical packet vectors. Verify Python and Node.js
against the Rust reference with `bash scripts/verify_vectors.sh` before relying
on an integration.

## Integration rules

1. Keep a datagram within the 1200-byte FCDP budget.
2. Validate packets before media processing.
3. Give each frame an expiration budget and discard late work.
4. Treat framing SDKs as framing only; they do not establish a secure session.
5. Do not expose a relay or gateway until your application supplies identity
   authorization, TLS termination where relevant, origin policy, and limits.

For exact fields and bytes, use the [protocol draft](../spec/fcdp-v0.1.md) and
the vectors under [`spec/`](../spec/).
