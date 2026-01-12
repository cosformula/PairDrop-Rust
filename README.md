# PairDrop-Rust

[PairDrop](https://github.com/schlagmichdoch/PairDrop) 的 Rust 后端重写版本，使用 Axum 框架。

## 为什么用 Rust？

| | Node.js | Rust |
|---|---------|------|
| 运行时内存 | ~60MB | ~2MB |
| 二进制大小 | 需要 Node.js 运行时 | 单个 ~5MB 二进制 |
| 启动时间 | 较慢 | 即时 |

## 功能

- ✅ 完全兼容原版前端
- ✅ WebSocket 信令服务器
- ✅ IP 自动发现（同网络设备互相发现）
- ✅ 设备配对（6位配对码）
- ✅ 公开房间（5字母房间码）
- ✅ WebRTC 信令转发
- ✅ WebSocket 回退（可选，用于 VPN 环境）
- ✅ 速率限制
- ✅ IPv6 支持

## 快速开始

### Docker (推荐)

```bash
docker run -d -p 3000:3000 ghcr.io/cosformula/pairdrop-rust:latest
```

### 从源码构建

```bash
cd server-rust
cargo build --release
./target/release/pairdrop-server
```

## 配置选项

```bash
# 命令行参数
pairdrop-server [OPTIONS]

Options:
  -p, --port <PORT>          端口 [默认: 3000]
      --ws-fallback          启用 WebSocket 回退
      --rate-limit <N>       启用速率限制
      --localhost-only       仅监听本地
      --debug-mode           调试模式
      --ipv6-localize <N>    IPv6 分段数 (1-7)

# 环境变量
PORT=3000
WS_FALLBACK=false
RATE_LIMIT=5
DEBUG_MODE=false
IPV6_LOCALIZE=4
```

## Docker 构建

```bash
# 构建
docker build --platform linux/amd64 -f server-rust/Dockerfile -t pairdrop-rust .

# 运行
docker run -d -p 3000:3000 pairdrop-rust
```

## 部署

支持部署到：
- Coolify
- Fly.io
- Railway
- 任何支持 Docker 的平台

## 致谢

- [PairDrop](https://github.com/schlagmichdoch/PairDrop) - 原版项目
- [Snapdrop](https://github.com/RobinLinus/snapdrop) - 最初的灵感来源

## License

GPL-3.0
