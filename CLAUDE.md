# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

PairDrop is a local file sharing web application inspired by Apple's AirDrop (fork of Snapdrop). It enables peer-to-peer file transfers via WebRTC between devices on the same local network, with additional features for internet transfers via paired devices and public rooms.

## Commands

### Development
```bash
npm install          # Install dependencies
npm start            # Start server on port 3000 (default)
PORT=8080 npm start  # Start server on custom port
```

### Production
```bash
npm run start:prod                    # With rate-limit and auto-restart
npm run start:prod -- --localhost-only  # Production behind reverse proxy
```

### Docker Development
```bash
docker compose -f docker-compose-dev.yml up --no-deps --build  # Dev server at localhost:8080
```

## Architecture

### Server-Side (`server/`)
- **index.js**: Entry point - parses environment variables, configures options, starts HTTP and WebSocket servers
- **server.js**: Express HTTP server serving static files from `public/` and `/config` endpoint
- **ws-server.js**: WebSocket signaling server managing rooms and peer discovery
- **peer.js**: Peer class representing connected clients with IP detection, rate limiting, and device naming

### Client-Side (`public/scripts/`)
- **network.js**: Core networking layer
  - `ServerConnection`: WebSocket connection to signaling server
  - `RTCPeer`: WebRTC peer connection for P2P transfers
  - `WSPeer`: WebSocket fallback when WebRTC unavailable
  - `PeersManager`: Manages all peer connections
  - `FileChunker`/`FileDigester`: Handle file chunking and reassembly
- **ui.js**: UI components (`PeersUI`, `PeerUI`, dialogs)
- **ui-main.js**: Main UI initialization and event handling
- **persistent-storage.js**: IndexedDB wrapper for paired devices/room secrets
- **localization.js**: i18n support loading from `public/lang/`
- **browser-tabs-connector.js**: Coordinates multiple PairDrop tabs in same browser
- **service-worker.js**: PWA service worker for offline support

### Room Types
The signaling server manages three room types for peer discovery:
1. **IP rooms**: Automatic discovery - peers on same IP/local network
2. **Secret rooms**: Paired devices using 256-char shared secrets
3. **Public rooms**: Temporary 5-letter room codes for internet transfer

### Communication Flow
1. Client connects via WebSocket to signaling server
2. Server assigns peer ID, joins client to IP-based room
3. Peers exchange WebRTC signaling messages (SDP/ICE) through server
4. Direct P2P connection established for file transfer
5. Files chunked (64KB chunks, 1MB partitions), transferred, and reassembled

## Key Environment Variables

- `PORT`: Server port (default: 3000)
- `DEBUG_MODE`: Enable debug logging for peer IPs
- `RATE_LIMIT`: Number of proxies for rate limiting trust
- `WS_FALLBACK`: Enable WebSocket fallback when WebRTC unavailable
- `RTC_CONFIG`: Path to JSON file with custom STUN/TURN servers
- `SIGNALING_SERVER`: Use external signaling server
- `IPV6_LOCALIZE`: IPv6 segment count for peer grouping (1-7)

## Contributing Guidelines

From CONTRIBUTING.md: PairDrop prioritizes radical simplicity. The main user flow must never be obstructed. Features are chosen carefully to avoid complexity. Stability comes first.
