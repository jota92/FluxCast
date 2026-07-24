# FluxCast（日本語）

FluxCast は、低遅延メディア配信向けの実験的な UDP 通信基盤です。視聴者に
届いても価値のない遅延映像を捨て、音声とキーフレームを優先する設計です。
**現在は pre-alpha であり、本番運用や機密情報の伝送には使わないでください。**
WebRTC、RTP、SRT、QUIC、MoQ との互換性はありません。

## まずローカルで試す

Rust の stable toolchain を [rustup](https://rustup.rs/) で導入してから、次を実行します。
クラウド契約、DB、カメラは不要です。

```sh
git clone <repository-url>
cd fluxcast
cargo test --workspace
cargo run -p fluxcast-cli -- demo
```

2つのターミナルで UDP 送受信も確認できます。

```sh
# ターミナル 1
cargo run -p fluxcast-cli -- receive 127.0.0.1:9000

# ターミナル 2
cargo run -p fluxcast-cli -- send 127.0.0.1:9000 "hello FluxCast"
```

## 取り入れ方

- **Rust:** `fluxcast-proto`、`fluxcast-core`、`fluxcast-security` を基に組み込みます。API はまだ安定化前です。
- **他言語:** [`sdk/`](../../sdk/) の最小実装と [`spec/test-vectors.json`](../../spec/test-vectors.json) を使って FCDP の入出力を確認します。Python と Node.js は CI でベクトル検証済みです。
- **ブラウザ:** [`gateway/README.md`](../../gateway/README.md) の WebSocket→UDP Gateway を使えます。公開時は必ず TLS、独自認証、Origin 制限、レート制限を設定してください。
- **カメラの実演:** [`demo/`](../../demo/) にカメラ・マイクから HLS 再生までのデモがあります。これは本番構成の雛形ではありません。

## 現在利用できる主な機能

- FCDP/UDP フラグメント化、期限切れフレームの破棄、再構成、XOR FEC、NACK
- 認証済み暗号化セッション（Ed25519、X25519、HKDF、ChaCha20-Poly1305、リプレイ拒否）
- STUN、認証済み ICE 接続チェックと nomination、TURN の Allocate/Permission/ChannelBind
- Relay、Prometheus/OpenMetrics、WebSocket→UDP Gateway
- H.264 Annex-B と Ogg Opus の CLI 送受信検証

未実装または未完了なのは、完全な ICE 状態機械・回線切替、ライブ Opus 実演、長時間の実回線性能測定、永続的 Relay 制御面、正式 SDK、第三者セキュリティ監査です。

## OSS と公開範囲

Apache-2.0 により、利用・変更・再配布・商用利用が可能です。必要なライセンス表示と通知は維持してください。商標の利用権、サポート契約、本番品質・安全性の保証は含まれません。詳しくは [LICENSE](../../LICENSE) と [GOVERNANCE.md](../../GOVERNANCE.md) を参照してください。

プロトコル変更や大きな機能追加は、まず Issue で相談してください。脆弱性は公開 Issue に書かず、[SECURITY.md](../../SECURITY.md) の手順に従ってください。
