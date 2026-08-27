# ADR-005: Safari WebTransport Stream compatibility transport

## 問題

iPhone Safariで `WebTransport` 接続は確立できたが、`WebTransport.datagrams` が公開されず、FCST Datagram送信を開始できなかった。

## 実験条件

正規のLet’s Encrypt証明書を使用し、`https://flexcast-studio.eastasia.cloudapp.azure.com/fc` へiPhone Safariから接続した。EdgeはHTTP/3/QUICの443/UDPで待ち受けた。

## 実測値

`WebTransport.ready` は成功した。続く `transport.datagrams.writable` の評価で `undefined` となった。EdgeへのTLS/QUIC接続は成立した。

## 原因

当該Safari実装はWebTransport Stream APIを提供するが、Datagram APIを提供しない部分実装である。

## 変更候補

Datagramを提供するブラウザは既存のFCST Datagram経路を使う。Safari互換経路だけは、Atom単位のStreamを作らず、1本の連続unidirectional WebTransport Streamに長さプレフィックス付きFCST Atomを多重化する。期限切れAtomを送らず、新しいREPAIR/REFRESHで収束する規則は保持する。

## 要件への影響

Safari互換経路は信頼性・輻輳制御がStream実装に従うため、Datagram経路と同じロス時の振る舞いを保証しない。Safariのブラウザのみを対象とした明示的な互換モードとし、標準経路のDatagram要件は変更しない。
