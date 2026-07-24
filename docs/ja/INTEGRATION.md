# コードへの組み込み方（日本語）

FluxCast は、必要な層だけを取り込める構成です。まず FCDP のフレーミングから
始め、必要に応じて Rust のメディアパイプラインや暗号化セッションを追加します。

## 目的別の選択

| 目的 | 使用するもの | 状態 |
| --- | --- | --- |
| FCDP パケットの生成・解析 | `fluxcast-proto` または `sdk/` | 利用可能、仕様は draft |
| 分割、FEC、NACK、期限管理 | `fluxcast-core` | Rust で利用可能 |
| 署名済みセッションと暗号化 | `fluxcast-security` | Rust で利用可能 |
| ブラウザから UDP への実験 | `gateway/` | TLS と独自アクセス制御が必要 |

## Rust

現時点では安定パッケージとしては公開していません。リポジトリを固定したうえで、
アプリケーションの `Cargo.toml` にローカルパスを指定します。

```toml
[dependencies]
fluxcast-proto = { path = "../FluxCast/crates/fluxcast-proto" }
fluxcast-core = { path = "../FluxCast/crates/fluxcast-core" }
fluxcast-security = { path = "../FluxCast/crates/fluxcast-security" }
```

`fluxcast-proto` はパケット形式、`fluxcast-core` はメディア送受信の振る舞い、
`fluxcast-security` はアプリケーション側で相手の identity を許可する方針を持つ場合に使います。

## Python と Node.js

いずれも依存のないフレーミング実装です。パッケージ配布はまだ行っていないため、
取得済みリポジトリから利用します。

```sh
PYTHONPATH="$PWD/sdk/python" python3 examples/python/send_fcdp.py 127.0.0.1 9000 hello
node examples/node/send-fcdp.mjs 127.0.0.1 9000 hello
```

Python は `fluxcast.fcdp`、Node.js は `sdk/node/fluxcast-fcdp.mjs` を読み込み、
`encode` と `decode` を使用します。

## Go、Swift、Kotlin、C

- **Go:** `sdk/go` の `fcdp.Encode` と `fcdp.Decode`
- **Swift:** `sdk/swift` をローカルパッケージとして追加し、`FCDP.encode` と `FCDP.decode`
- **Kotlin:** `sdk/kotlin/src/main/kotlin/FluxCastFcdp.kt` をアプリモジュールに含め、`FluxCastFcdp.encode` と `decode`
- **C:** `examples/c/send_fcdp.c` は POSIX 向けの単一パケット例です。

```sh
cc -O2 examples/c/send_fcdp.c -o /tmp/fluxcast-send
/tmp/fluxcast-send 127.0.0.1 9000 hello
```

## 守るべきこと

1. 1 つの FCDP データグラムを 1200 バイト以内にします。
2. メディア処理の前に入力を検証します。
3. フレーミング SDK だけで安全なセッションが確立されるわけではありません。
4. Gateway や Relay を公開する場合は、認証、TLS、Origin 制限、レート制限をアプリケーション側で用意します。

正確なバイト形式は [FCDP 仕様](../../spec/fcdp-v0.1.md) と `spec/` のベクトルを参照してください。
