# はじめに（日本語）

この手順では、初回取得からローカル UDP の送受信確認までを行います。アカウント、
外部サーバー、DB、カメラは不要です。

## 必要なもの

- stable Rust
- Git
- 任意: SDK ベクトル検証用の Node.js 22+ と Python 3

## 取得と動作確認

```sh
git clone https://github.com/jota92/FluxCast.git
cd FluxCast
cargo test --workspace
cargo run -p fluxcast-cli -- demo
```

最後のコマンドは同じ PC 内の UDP 送受信だけを使います。

## 2つのターミナルで送受信する

```sh
# ターミナル 1
cargo run -p fluxcast-cli -- receive 127.0.0.1:9000

# ターミナル 2
cargo run -p fluxcast-cli -- send 127.0.0.1:9000 "hello FluxCast"
```

受信できない場合は、UDP 9000 番ポートを別のアプリが使っていないか確認してください。

## 次に読む資料

- [コードへの組み込み方](INTEGRATION.md)
- [Gateway と実験環境の運用](OPERATIONS.md)
- [カメラのデモ](../../demo/README.md)
- [FCDP 仕様](../../spec/fcdp-v0.1.md)

FluxCast は pre-alpha です。機密情報や本番運用には使用しないでください。
