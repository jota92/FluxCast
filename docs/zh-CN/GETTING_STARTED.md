# 快速开始（简体中文）

本指南从首次获取项目到验证本机 UDP 收发。无需账号、远程主机、数据库或摄像头。

## 前提条件

- stable Rust
- Git
- 可选：Node.js 22+ 与 Python 3，用于 SDK 向量验证

## 获取并验证

```sh
git clone https://github.com/jota92/FluxCast.git
cd FluxCast
cargo test --workspace
cargo run -p fluxcast-cli -- demo
```

最后一条命令只使用本机回环 UDP，不会连接互联网。

## 用两个终端收发数据

```sh
# 终端 1
cargo run -p fluxcast-cli -- receive 127.0.0.1:9000

# 终端 2
cargo run -p fluxcast-cli -- send 127.0.0.1:9000 "hello FluxCast"
```

如果没有收到消息，请确认没有其他程序占用 UDP 端口 9000。

## 下一步

- [代码集成](INTEGRATION.md)
- [Gateway 与实验运维](OPERATIONS.md)
- [摄像头演示](../../demo/README.md)
- [FCDP 规范](../../spec/fcdp-v0.1.md)

FluxCast 仍处于 pre-alpha 阶段，请勿用于机密媒体或生产环境。
