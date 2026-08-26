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

カメラが1080p30を公開しない端末では、送信ページはその実解像度を取得して60×45の論理Visual Stateへ正規化する**互換モード**で開始できます。Edgeの出力契約は1920×1080のままです。FHD30を実測する評価では、送信ページの表示が `FHD30 Ready` である端末だけを採用してください。

## iPhoneでの開発用証明書の信頼

WebTransportは、Safariで表示された警告を通過しただけの自己署名証明書には接続できません。ローカルCAを使用するデモでは、CA証明書を渡すために `FLEXCAST_DEMO_CA_CERT=/absolute/path/FlexCast-Demo-CA.cer` をStudio起動時に追加します。iPhoneで `https://<LAN-IP>:3030/flexcast-demo-ca.cer` を開き、次を一度だけ行います。

1. ダウンロードしたプロファイルを「設定」→「一般」→「VPNとデバイス管理」からインストールする。
2. 「設定」→「一般」→「情報」→「証明書信頼設定」で **FlexCast Demo Development CA** のフル信頼を有効にする。

その後QRをもう一度開きます。このCAは開発ネットワーク専用です。秘密鍵やCA証明書を公開サーバーへ配置しないでください。

## ポート

| 用途 | ポート | プロトコル |
| --- | ---: | --- |
| Studio / QR / 送信ページ | 3030 | HTTPS/TCP |
| Edge Media | 4433 | HTTPS over QUIC/UDP |
| Edge preview | 3031 | PCローカルのみ |

`3031` は `127.0.0.1` にしか待ち受けません。外部へ開放するのは3030/TCPと4433/UDPだけです。

## このデモの範囲

これは開発検証用です。招待トークンはQRの経路選択に使うだけで、現時点のEdgeはまだ署名済み招待の検証を強制しません。公開インターネットへ常設する前に、FCST制御面の認証・期限・失効をEdge側へ接続する必要があります。
