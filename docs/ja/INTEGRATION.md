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

接続候補は、優先順に `IceAgent::nominate_first_reachable` へ渡します。各候補を
認証済みチェックで再試行し、最初に到達した経路を nomination して RTT を返します。
両端が同じ role で開始した場合は tie-breaker で負けた側だけが role を変更します。
チェックを再試行するか、候補ごとの試行回数を 2 回以上にして helper を使います。
`IceAgent::restart` は、認証済みのシグナリング経路で新しい資格情報を交換してから使います。

暗号化済みデータ経路の切替には、`SecurePathConfig` と `SecurePathEndpoint` を使います。
`probe` は暗号化された到達確認を送り、有効な応答を受けたときだけ `active()` を変更します。
この endpoint 経由のメディアは `send_media` で送り、送信シーケンス番号を endpoint に管理させます。

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

`bash scripts/verify_vectors.sh` は Rust、Python、Node.js、Go、Swift の共通ベクトルを検証します。Kotlin も検証する環境では、`FLUXCAST_VERIFY_KOTLIN=1 bash scripts/verify_vectors.sh` を実行してください。Kotlin の実行には 120 秒の上限があります。
