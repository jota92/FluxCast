# Gateway 与实验运维（简体中文）

本指南仅面向受控实验环境，不代表生产服务安全保证。

## 角色

| 组件 | 职责 |
| --- | --- |
| Publisher | 创建媒体和应用策略 |
| Relay | 向已授权观看者转发受保护的数据包 |
| Gateway | 在浏览器 WebSocket 与 UDP 之间转发 |
| Receiver | 重组、验证与播放 |

Relay 和 Gateway 不负责 identity、授权或 TLS；这些必须由应用提供。

## 本地 Gateway

```sh
FLUXCAST_GATEWAY_TOKEN='足够长的随机值' \
FLUXCAST_UDP_PEER='127.0.0.1:19100' \
node gateway/fluxcast-gateway.mjs
```

Gateway 只绑定 loopback。若要对外提供，必须在前面配置 TLS、Origin 白名单、认证、
短期令牌、限流和请求大小限制。不要把令牌写入 URL、源码或日志。

## 网络实验

- 只开放实验所需的 UDP 端口。
- 将来源限制为已知的 Publisher / Receiver。
- 使用临时凭据并在结束后撤销。
- 记录丢包、过期丢弃、重传和 Relay 订阅数。
- 实验结束后移除防火墙规则和凭据。

开始前请运行[快速开始](GETTING_STARTED.md)中的验证，并阅读 [SECURITY.md](../../SECURITY.md)。
