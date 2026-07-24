# 代码集成（简体中文）

FluxCast 可按需接入不同层。建议先从 FCDP 封包开始，再按需要加入 Rust 媒体管线或加密会话。

## 按需求选择

| 需求 | 使用内容 | 状态 |
| --- | --- | --- |
| FCDP 数据包编码/解码 | `fluxcast-proto` 或 `sdk/` | 可用，规范仍为 draft |
| 分片、FEC、NACK、时限管理 | `fluxcast-core` | Rust 可用 |
| 已签名会话和加密 | `fluxcast-security` | Rust 可用 |
| 浏览器到 UDP 的实验 | `gateway/` | 需要自有 TLS 与访问控制 |

## Rust

当前尚未作为稳定包发布。固定仓库版本后，在应用的 `Cargo.toml` 中使用本地路径：

```toml
[dependencies]
fluxcast-proto = { path = "../FluxCast/crates/fluxcast-proto" }
fluxcast-core = { path = "../FluxCast/crates/fluxcast-core" }
fluxcast-security = { path = "../FluxCast/crates/fluxcast-security" }
```

`fluxcast-proto` 负责数据包格式，`fluxcast-core` 负责媒体传输行为；只有当应用具有明确的对端 identity 授权策略时才使用 `fluxcast-security`。

将候选地址按优先级传给 `IceAgent::nominate_first_reachable`。它会使用已认证检查重试每个候选、提名第一个可达路径，并返回 RTT。只有通过应用的已认证信令交换新凭据后，才可调用 `IceAgent::restart`。

若双方以相同 role 开始，tie-breaker 只会改变失败一方的 role。请重新检查，或让 helper 对每个候选至少尝试两次。

加密数据路径切换使用 `SecurePathConfig` 与 `SecurePathEndpoint`。`probe` 会发送加密连通性探测，只有收到匹配的有效响应时才改变 `active()`。通过该 endpoint 发送媒体时使用 `send_media`，以便由 endpoint 管理全部出站序列号。

## Python 与 Node.js

两者都是无依赖的封包实现，尚未单独发布包。请从已获取的仓库中使用：

```sh
PYTHONPATH="$PWD/sdk/python" python3 examples/python/send_fcdp.py 127.0.0.1 9000 hello
node examples/node/send-fcdp.mjs 127.0.0.1 9000 hello
```

Python 使用 `fluxcast.fcdp`，Node.js 使用 `sdk/node/fluxcast-fcdp.mjs`，均提供 `encode` 与 `decode`。

## Go、Swift、Kotlin 与 C

- **Go：**`sdk/go` 的 `fcdp.Encode` 和 `fcdp.Decode`
- **Swift：**把 `sdk/swift` 作为本地包，使用 `FCDP.encode` 和 `FCDP.decode`
- **Kotlin：**将 `sdk/kotlin/src/main/kotlin/FluxCastFcdp.kt` 纳入应用模块
- **C：**`examples/c/send_fcdp.c` 是 POSIX 单数据包示例。

```sh
cc -O2 examples/c/send_fcdp.c -o /tmp/fluxcast-send
/tmp/fluxcast-send 127.0.0.1 9000 hello
```

## 集成规则

1. 单个 FCDP 数据报不得超过 1200 字节。
2. 在媒体处理之前验证输入。
3. 封包 SDK 本身不会建立安全会话。
4. 对外部署 Gateway 或 Relay 时，应用必须提供认证、TLS、Origin 限制与限流。

精确字节格式请看 [FCDP 规范](../../spec/fcdp-v0.1.md) 与 `spec/` 中的向量文件。
