# iPhone向け FluxCast Camera

このXcodeプロジェクトは、iPhone背面カメラの映像を端末内のH.264ハードウェア
エンコーダで圧縮し、FCDP/UDPでMacへ送信します。まず同一Wi-Fi内で送受信を
確認するための実演アプリです。暗号化済みセッション、NAT越え、音声はこの実演の
範囲には含まれません。そのため、信頼できるローカルネットワーク内だけで使ってください。

## このMacで受信する

まずMacで次を実行します。H.264を表示できるFFmpegが必要です。

```sh
cargo run -p fluxcast-cli -- receive-h264 0.0.0.0:19100 | \
  ffplay -fflags nobuffer -flags low_delay -framedrop -f h264 -
```

次に、このMacのWi-Fi IPアドレス（例: `192.168.1.10`）を確認します。
`FluxCastCamera.xcodeproj` をXcodeで開き、署名できる実機iPhoneを選んで実行します。
アプリにMacのIPアドレスとポート `19100` を入力して、**Start camera stream** を押してください。

初回起動時はカメラとローカルネットワークへのアクセスを許可します。iPhoneとMacは
同じWi-Fiに接続してください。iOS Simulatorには物理カメラがないため、送信確認には
実機が必要です。
