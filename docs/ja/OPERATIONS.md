# Gateway と実験環境の運用（日本語）

この資料は管理下の実験環境向けです。本番サービスとしての安全性を保証するものではありません。

## 役割

| 要素 | 担当 |
| --- | --- |
| Publisher | メディアの生成とアプリケーション方針 |
| Relay | 許可された視聴者への保護済みパケット転送 |
| Gateway | ブラウザの WebSocket と UDP の間の転送 |
| Receiver | 再構成、検証、表示 |

Relay と Gateway に identity・認可・TLS の責務はありません。アプリケーション側で用意してください。

## ローカル Gateway

```sh
FLUXCAST_GATEWAY_TOKEN='十分に長いランダム値' \
FLUXCAST_UDP_PEER='127.0.0.1:19100' \
node gateway/fluxcast-gateway.mjs
```

Gateway は loopback にだけ bind します。公開する場合は TLS 終端、Origin 許可、
認証、短い有効期限のトークン、レート制限、リクエストサイズ制限を前段で設定してください。
トークンを URL、ソース、ログへ残してはいけません。

## ネットワーク実験

- 実験に必要な UDP ポートだけを開けます。
- 接続元を既知の Publisher / Receiver に限定します。
- 一時的な資格情報を使い、実験後に無効化します。
- ロス、遅延破棄、再送、Relay 購読数を記録します。
- `fluxcast_relay_send_failures_total` を記録します。ローカル送信失敗が同じ購読者で
  3 回連続すると、その購読者だけを除外し、残りの視聴者への転送は継続します。
- 実験終了後にファイアウォール規則と資格情報を削除します。

実験前に [はじめに](GETTING_STARTED.md) の検証を実行し、
[SECURITY.md](../../SECURITY.md) の未解決事項を確認してください。
