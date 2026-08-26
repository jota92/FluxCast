# PC・スマートフォン デモ

このデモは、PCのStudio画面で発行したQRコードをスマートフォンで読み込み、ブラウザだけでカメラをFlexCast Edgeへ送信し、PCでVisual Stateを確認するための最小の動作経路です。

`localhost` はスマートフォンではスマートフォン自身を指すため、QRコードにはPC/Edgeから到達できる**LAN IPまたは公開DNS名のHTTPS URL**を設定します。PC側は同じサーバーを `https://localhost:3030` で開いてかまいません。

## 起動

1. TLS証明書を、QRに入れるホスト名またはLAN IPのSAN付きで用意し、スマートフォンがその発行元を信頼するようにします。開発用の自己署名証明書では、iPhone/AndroidにCAを信頼させる必要があります。
2. 依存関係とブラウザ送信画面を準備します。

```sh
pnpm install
pnpm build
```

3. Edgeを起動します。証明書と鍵はStudioと同じホスト名/IPを使えます。

```sh
cargo run -p edge-gateway -- --port 4433 --preview-port 3031 --cert /absolute/path/cert.pem --key /absolute/path/key.pem
```

4. 別の端末でStudioを起動します。ここではPCのLAN IPを `192.168.3.8` とした例です。

```sh
FLEXCAST_DEMO_ORIGIN=https://192.168.3.8:3030 \
FLEXCAST_EDGE_URL=https://192.168.3.8:4433/fc \
FLEXCAST_TLS_CERT=/absolute/path/cert.pem \
FLEXCAST_TLS_KEY=/absolute/path/key.pem \
pnpm --filter @flexcast/demo-server start
```

5. PCで `https://localhost:3030` または `https://192.168.3.8:3030` を開き、**新しいカメラ招待を作成**します。スマートフォンでQRを読み込み、カメラとマイクを許可し、表示される画面で配信を開始します。

## ポート

| 用途 | ポート | プロトコル |
| --- | ---: | --- |
| Studio / QR / 送信ページ | 3030 | HTTPS/TCP |
| Edge Media | 4433 | HTTPS over QUIC/UDP |
| Edge preview | 3031 | PCローカルのみ |

`3031` は `127.0.0.1` にしか待ち受けません。外部へ開放するのは3030/TCPと4433/UDPだけです。

## このデモの範囲

これは開発検証用です。招待トークンはQRの経路選択に使うだけで、現時点のEdgeはまだ署名済み招待の検証を強制しません。公開インターネットへ常設する前に、FCST制御面の認証・期限・失効をEdge側へ接続する必要があります。
