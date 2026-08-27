# ADR-006: Visual Surface compression is required for smooth FHD30

## 問題

Safari互換のFCST Stream経路で、32×24 RGB Surfaceをそのまま送ると、全Regionは正しく到達する一方、動く映像にRegion単位の時間差が見える。実機プレビューでは停止・カクつき・ぼやけとして知覚される。

## 実験条件

公開Edge `https://flexcast-studio.eastasia.cloudapp.azure.com/fc` に対し、1本のunidirectional WebTransport Streamで、2700 Regionの初期Surfaceと90フレームの動的Surfaceを送った。帯域予算はSafari Senderと同じおよそ16〜20Mbpsとした。EdgeのRGBAプレビューをPNG化して視覚確認した。

## 実測値

- 5850 Atom受信、5850 Atom反映
- invalid/replayed Atom: 0
- p95 Region Age: 2.6秒
- FHD RGB 4:4:4の非圧縮帯域: `1920 × 1080 × 3 × 8 × 30 = 1.49Gbps`

全Regionの順序・描画は正しいが、更新フレームが異なるRegionが同時に表示される裂け目を確認した。

## 原因

現在のRAW Surfaceは1 Regionあたり2305 byteで、20Mbpsでは毎フレームに更新できるのはおよそ35 Regionだけである。これは伝送・ロック・連番のバグではなく、非圧縮画素量と帯域予算の差による。

## 変更候補

標準Video Codecや既存メディアプロトコルには置き換えない。FCST Visual Atomとして、次を満たす独自の空間圧縮Surfaceを実装する。

- Region内の予測・変換・量子化・可変長符号化
- 低周波Surfaceを全Regionへ連続供給し、高周波Detailを優先度付きで追加
- Motion/REPAIR Atomで空間更新の時間差を抑制
- Edgeでの独自デコードと、Visual Stateへの合成

## 要件への影響

WebRTC、RTP/RTCP、WHIP、SRT、RTMP、HLS、MPEG-DASH、およびH.264/HEVC/AV1/VP8/VP9は導入しない。FlexCastのVisual State方式を維持したまま、RAW Surfaceのみを最初の正しさ確認用経路と位置付ける。
