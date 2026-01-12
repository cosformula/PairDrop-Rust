# PairDrop-Rust

> **Modified Version Notice (GPLv3 §5a)**
> This is a modified version of [PairDrop](https://github.com/schlagmichdoch/PairDrop).
> Modification: Complete backend rewrite from Node.js to Rust (Axum framework).
> Initial modification date: January 2026.
> Original authors: [schlagmichdoch](https://github.com/schlagmichdoch) and contributors.
> See [NOTICE](NOTICE) for full attribution.

A Rust rewrite of [PairDrop](https://github.com/schlagmichdoch/PairDrop) backend using the Axum framework.

**Demo: [pairdrop-rust.app.cosformula.org](https://pairdrop-rust.app.cosformula.org/)**

## Why Rust?

| | Node.js | Rust |
|---|---------|------|
| Runtime Memory | ~60MB | ~2MB |
| Binary Size | Requires Node.js runtime | Single ~5MB binary |
| Startup Time | Slow | Instant |

## Features

- ✅ Fully compatible with original frontend
- ✅ WebSocket signaling server
- ✅ Auto-discovery (devices on same network)
- ✅ Device pairing (6-digit code)
- ✅ Public rooms (5-letter room code)
- ✅ WebRTC signaling relay
- ✅ WebSocket fallback (optional, for VPN environments)
- ✅ Rate limiting
- ✅ IPv6 support

## Quick Start

### Docker (Recommended)

```bash
docker run -d -p 3000:3000 ghcr.io/cosformula/pairdrop-rust:latest
```

### Build from Source

```bash
cd server-rust
cargo build --release
./target/release/pairdrop-server
```

## Configuration

```bash
# Command line options
pairdrop-server [OPTIONS]

Options:
  -p, --port <PORT>          Port [default: 3000]
      --ws-fallback          Enable WebSocket fallback
      --rate-limit <N>       Enable rate limiting
      --localhost-only       Listen on localhost only
      --debug-mode           Enable debug mode
      --ipv6-localize <N>    IPv6 segments (1-7)

# Environment variables
PORT=3000
WS_FALLBACK=false
RATE_LIMIT=5
DEBUG_MODE=false
IPV6_LOCALIZE=4
```

## Docker Build

```bash
# Build
docker build --platform linux/amd64 -f server-rust/Dockerfile -t pairdrop-rust .

# Run
docker run -d -p 3000:3000 pairdrop-rust
```

### Source Code for Docker Images (GPLv3 §6)

| Docker Image Tag | Source Code |
|------------------|-------------|
| `ghcr.io/cosformula/pairdrop-rust:latest` | [main branch](https://github.com/cosformula/pairdrop-rust) |
| `ghcr.io/cosformula/pairdrop-rust:vX.Y.Z` | [Tag vX.Y.Z](https://github.com/cosformula/pairdrop-rust/releases) |

## Deployment

Supports deployment to:
- Coolify
- Fly.io
- Railway
- Any Docker-compatible platform

## Credits

- [PairDrop](https://github.com/schlagmichdoch/PairDrop) - Original project
- [Snapdrop](https://github.com/RobinLinus/snapdrop) - Original inspiration

## License

GPL-3.0
