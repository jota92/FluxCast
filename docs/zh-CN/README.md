# FluxCast（简体中文）

FluxCast 是一个面向低延迟媒体传输的实验性 UDP 基础设施。它优先传输音频和
关键帧，并丢弃已经来不及改善观看体验的视频。**项目目前处于 pre-alpha 阶段，
请勿用于生产环境或机密媒体。**FCDP 的数据包格式和公开 API 在稳定版本前仍可能变化。

## 本地快速开始

先通过 [rustup](https://rustup.rs/) 安装稳定版 Rust。以下演示不需要云账号、数据库
或摄像头。

```sh
git clone <repository-url>
cd fluxcast
cargo test --workspace
cargo run -p fluxcast-cli -- demo
```

更完整的步骤请看[快速开始](GETTING_STARTED.md)。

也可用两个终端确认本机 UDP 收发：

```sh
# 终端 1
cargo run -p fluxcast-cli -- receive 127.0.0.1:9000

# 终端 2
cargo run -p fluxcast-cli -- send 127.0.0.1:9000 "hello FluxCast"
```

## 如何集成

- **Rust：**以 `fluxcast-proto`、`fluxcast-core` 和 `fluxcast-security` 为基础接入；API 尚未稳定。
- **其他语言：**使用 [`sdk/`](../../sdk/) 的最小实现和 [`spec/test-vectors.json`](../../spec/test-vectors.json) 验证 FCDP 编解码。Python 与 Node.js 可通过仓库附带脚本完成向量验证。
- **浏览器：**可使用 [`gateway/README.md`](../../gateway/README.md) 中的 WebSocket→UDP Gateway。对外部署前必须配置 TLS、应用自己的认证、Origin 限制和限流。
- **摄像头演示：**[`demo/`](../../demo/) 提供从摄像头和麦克风到 HLS 播放的演示；它不是生产部署模板。

详细资料包括[代码集成](INTEGRATION.md)、[Gateway 与实验运维](OPERATIONS.md)和[文档索引（英文）](../README.md)。

## 当前已有的能力

- FCDP/UDP 分片、过期帧丢弃、重组、XOR FEC 和 NACK
- 已认证加密会话：Ed25519、X25519、HKDF、ChaCha20-Poly1305 与重放拒绝
- STUN、已认证 ICE 连通性检查与 nomination，以及 TURN Allocate/Permission/ChannelBind
- Relay、Prometheus/OpenMetrics、WebSocket→UDP Gateway
- H.264 Annex-B 和 Ogg Opus 的 CLI 收发验证

完整 ICE 状态机与网络切换、实时 Opus 演示、长时间真实网络性能测试、持久化 Relay 控制面、正式 SDK 和独立安全审计仍未完成。

## 开源许可与公开范围

项目使用 Apache-2.0：允许使用、修改、再发布和商业使用，但必须保留所需的许可证和通知。它不授予商标使用权、支持合同，也不保证生产质量或安全性。详情请看 [LICENSE](../../LICENSE) 和 [GOVERNANCE.md](../../GOVERNANCE.md)。

提交大规模协议或功能改动前，请先创建 Issue 讨论。安全漏洞请勿公开提交；请遵循 [SECURITY.md](../../SECURITY.md)。
