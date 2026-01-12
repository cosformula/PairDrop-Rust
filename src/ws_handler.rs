use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::interval;

use crate::config::{AppConfig, WsConfig};
use crate::peer::Peer;
use crate::rooms::{RoomManager, RoomType};

/// Keep-alive timer info
struct KeepAliveInfo {
    last_beat: Instant,
}

/// WebSocket handler state
pub struct WsHandler {
    config: Arc<AppConfig>,
    rooms: Arc<RoomManager>,
    keep_alive_timers: Mutex<HashMap<String, KeepAliveInfo>>,
}

impl WsHandler {
    pub fn new(config: Arc<AppConfig>, rooms: Arc<RoomManager>) -> Arc<Self> {
        Arc::new(Self {
            config,
            rooms,
            keep_alive_timers: Mutex::new(HashMap::new()),
        })
    }

    /// Handle a new WebSocket connection
    pub async fn handle_connection(
        self: Arc<Self>,
        socket: WebSocket,
        addr: Option<SocketAddr>,
        headers: axum::http::HeaderMap,
        query: String,
    ) {
        // Parse query params
        let query_params: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();

        // Create channel for sending messages to WebSocket
        let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

        // Create peer
        let peer = Peer::new(
            tx,
            addr.map(|a| a.ip()),
            &headers,
            &query_params,
            self.config.ipv6_localize,
            self.config.debug_mode,
        );

        // Initialize keep-alive
        self.keep_alive_timers.lock().insert(
            peer.id.clone(),
            KeepAliveInfo {
                last_beat: Instant::now(),
            },
        );

        // Send ws-config
        let ws_config = WsConfig {
            rtc_config: self.config.rtc_config.clone(),
            ws_fallback: self.config.ws_fallback,
        };
        let ws_config_msg = serde_json::json!({
            "type": "ws-config",
            "wsConfig": ws_config
        });
        peer.send_json(&ws_config_msg);

        // Send display-name
        let display_name_msg = serde_json::json!({
            "type": "display-name",
            "displayName": peer.name.display_name,
            "deviceName": peer.name.device_name,
            "peerId": peer.id,
            "peerIdHash": peer.get_peer_id_hash()
        });
        peer.send_json(&display_name_msg);

        // Split socket
        let (mut ws_sender, mut ws_receiver) = socket.split();

        // Clone for tasks
        let peer_clone = Arc::clone(&peer);
        let handler_clone = Arc::clone(&self);
        let peer_id = peer.id.clone();

        // Spawn task to forward messages from channel to WebSocket
        let send_task = tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if ws_sender.send(msg).await.is_err() {
                    break;
                }
            }
        });

        // Spawn keep-alive task
        let peer_for_keepalive = Arc::clone(&peer);
        let handler_for_keepalive = Arc::clone(&self);
        let keepalive_task = tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(1));
            loop {
                ticker.tick().await;

                let should_disconnect = {
                    let timers = handler_for_keepalive.keep_alive_timers.lock();
                    if let Some(info) = timers.get(&peer_for_keepalive.id) {
                        info.last_beat.elapsed() > Duration::from_secs(5)
                    } else {
                        true
                    }
                };

                if should_disconnect {
                    break;
                }

                // Send ping
                let ping_msg = serde_json::json!({"type": "ping"});
                if !peer_for_keepalive.send_json(&ping_msg) {
                    break;
                }
            }
        });

        // Handle incoming messages
        while let Some(result) = ws_receiver.next().await {
            match result {
                Ok(Message::Text(text)) => {
                    handler_clone.handle_message(&peer_clone, &text).await;
                }
                Ok(Message::Binary(_)) => {
                    // Binary messages not expected from client
                }
                Ok(Message::Close(_)) => {
                    break;
                }
                Err(e) => {
                    tracing::error!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        // Cleanup on disconnect
        self.disconnect(&peer);
        self.keep_alive_timers.lock().remove(&peer_id);

        // Abort tasks
        send_task.abort();
        keepalive_task.abort();
    }

    /// Handle an incoming message
    async fn handle_message(&self, peer: &Arc<Peer>, text: &str) {
        let msg: Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("WS: Received malformed JSON: {}", e);
                return;
            }
        };

        let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match msg_type {
            "disconnect" => {
                self.disconnect(peer);
            }
            "pong" => {
                self.set_keep_alive_timer_to_now(peer);
            }
            "join-ip-room" => {
                self.on_join_ip_room(peer);
            }
            "room-secrets" => {
                self.on_room_secrets(peer, &msg);
            }
            "room-secrets-deleted" => {
                self.on_room_secrets_deleted(&msg);
            }
            "pair-device-initiate" => {
                self.on_pair_device_initiate(peer);
            }
            "pair-device-join" => {
                self.on_pair_device_join(peer, &msg);
            }
            "pair-device-cancel" => {
                self.on_pair_device_cancel(peer);
            }
            "regenerate-room-secret" => {
                self.on_regenerate_room_secret(&msg);
            }
            "create-public-room" => {
                self.on_create_public_room(peer);
            }
            "join-public-room" => {
                self.on_join_public_room(peer, &msg);
            }
            "leave-public-room" => {
                self.on_leave_public_room(peer);
            }
            "signal" => {
                self.on_signal(peer, msg);
            }
            // WS fallback relay messages
            "request" | "header" | "partition" | "partition-received" | "progress"
            | "files-transfer-response" | "file-transfer-complete" | "message-transfer-complete"
            | "text" | "display-name-changed" | "ws-chunk" => {
                if self.config.ws_fallback {
                    self.on_signal(peer, msg);
                } else {
                    tracing::warn!("Websocket fallback is not activated on this instance.");
                }
            }
            _ => {
                tracing::warn!("WS: Unknown message type: {}", msg_type);
            }
        }
    }

    /// Disconnect a peer
    fn disconnect(&self, peer: &Arc<Peer>) {
        // Remove pair key
        if let Some(pair_key) = peer.pair_key.lock().take() {
            self.rooms.remove_pair_key(&pair_key);
        }

        // Leave all rooms
        self.rooms.leave_ip_room(peer, true);
        self.rooms.leave_all_secret_rooms(peer, true);
        self.rooms.leave_public_room(peer, true);
    }

    /// Update keep-alive timer
    fn set_keep_alive_timer_to_now(&self, peer: &Arc<Peer>) {
        if let Some(info) = self.keep_alive_timers.lock().get_mut(&peer.id) {
            info.last_beat = Instant::now();
        }
    }

    /// Handle join-ip-room message
    fn on_join_ip_room(&self, peer: &Arc<Peer>) {
        let existing_peers = self.rooms.join_ip_room(peer);

        // Send peers list to the joining peer
        let peers_msg = serde_json::json!({
            "type": "peers",
            "peers": existing_peers,
            "roomType": "ip",
            "roomId": peer.ip
        });
        peer.send_json(&peers_msg);
    }

    /// Handle room-secrets message
    fn on_room_secrets(&self, peer: &Arc<Peer>, msg: &Value) {
        let room_secrets = msg
            .get("roomSecrets")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .filter(|s| {
                        // Validate: 64-256 ASCII printable chars
                        (64..=256).contains(&s.len()) && s.chars().all(|c| c.is_ascii())
                    })
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        for room_secret in room_secrets {
            let existing_peers = self.rooms.join_secret_room(peer, &room_secret);

            // Send peers list
            let peers_msg = serde_json::json!({
                "type": "peers",
                "peers": existing_peers,
                "roomType": "secret",
                "roomId": room_secret
            });
            peer.send_json(&peers_msg);
        }
    }

    /// Handle room-secrets-deleted message
    fn on_room_secrets_deleted(&self, msg: &Value) {
        let room_secrets = msg
            .get("roomSecrets")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        for room_secret in room_secrets {
            self.rooms.delete_secret_room(&room_secret);
        }
    }

    /// Handle pair-device-initiate message
    fn on_pair_device_initiate(&self, peer: &Arc<Peer>) {
        let (room_secret, pair_key) = self.rooms.pair_device_initiate(peer);

        let msg = serde_json::json!({
            "type": "pair-device-initiated",
            "roomSecret": room_secret,
            "pairKey": pair_key
        });
        peer.send_json(&msg);
    }

    /// Handle pair-device-join message
    fn on_pair_device_join(&self, peer: &Arc<Peer>, msg: &Value) {
        if peer.rate_limit_reached() {
            let rate_limit_msg = serde_json::json!({"type": "join-key-rate-limit"});
            peer.send_json(&rate_limit_msg);
            return;
        }

        let pair_key = msg.get("pairKey").and_then(|v| v.as_str()).unwrap_or("");

        if let Some((room_secret, creator_id)) = self.rooms.pair_device_join(peer, pair_key) {
            // Notify joiner
            let joiner_msg = serde_json::json!({
                "type": "pair-device-joined",
                "roomSecret": room_secret,
                "peerId": creator_id
            });
            peer.send_json(&joiner_msg);

            // Notify creator
            if let Some(creator) = self.rooms.get_peer(&creator_id) {
                let creator_msg = serde_json::json!({
                    "type": "pair-device-joined",
                    "roomSecret": room_secret,
                    "peerId": peer.id
                });
                creator.send_json(&creator_msg);
            }
        } else {
            let invalid_msg = serde_json::json!({"type": "pair-device-join-key-invalid"});
            peer.send_json(&invalid_msg);
        }
    }

    /// Handle pair-device-cancel message
    fn on_pair_device_cancel(&self, peer: &Arc<Peer>) {
        if let Some(pair_key) = self.rooms.pair_device_cancel(peer) {
            let msg = serde_json::json!({
                "type": "pair-device-canceled",
                "pairKey": pair_key
            });
            peer.send_json(&msg);
        }
    }

    /// Handle regenerate-room-secret message
    fn on_regenerate_room_secret(&self, msg: &Value) {
        if let Some(room_secret) = msg.get("roomSecret").and_then(|v| v.as_str()) {
            self.rooms.regenerate_room_secret(room_secret);
        }
    }

    /// Handle create-public-room message
    fn on_create_public_room(&self, peer: &Arc<Peer>) {
        let room_id = self.rooms.create_public_room(peer);

        let msg = serde_json::json!({
            "type": "public-room-created",
            "roomId": room_id
        });
        peer.send_json(&msg);

        // No need to send peers list, join_public_room already does that
    }

    /// Handle join-public-room message
    fn on_join_public_room(&self, peer: &Arc<Peer>, msg: &Value) {
        if peer.rate_limit_reached() {
            let rate_limit_msg = serde_json::json!({"type": "join-key-rate-limit"});
            peer.send_json(&rate_limit_msg);
            return;
        }

        let room_id = msg
            .get("publicRoomId")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let create_if_invalid = msg
            .get("createIfInvalid")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !self.rooms.public_room_exists(room_id) && !create_if_invalid {
            let invalid_msg = serde_json::json!({
                "type": "public-room-id-invalid",
                "publicRoomId": room_id
            });
            peer.send_json(&invalid_msg);
            return;
        }

        let existing_peers = self.rooms.join_public_room(peer, room_id);

        // Send peers list
        let peers_msg = serde_json::json!({
            "type": "peers",
            "peers": existing_peers,
            "roomType": "public-id",
            "roomId": room_id
        });
        peer.send_json(&peers_msg);
    }

    /// Handle leave-public-room message
    fn on_leave_public_room(&self, peer: &Arc<Peer>) {
        self.rooms.leave_public_room(peer, true);

        let msg = serde_json::json!({"type": "public-room-left"});
        peer.send_json(&msg);
    }

    /// Handle signal message (WebRTC signaling relay)
    fn on_signal(&self, peer: &Arc<Peer>, msg: Value) {
        let to = msg
            .get("to")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let room_type_str = msg
            .get("roomType")
            .and_then(|v| v.as_str())
            .unwrap_or("ip")
            .to_string();
        let room_id_opt = msg
            .get("roomId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let room_type = match room_type_str.as_str() {
            "ip" => RoomType::Ip,
            "secret" => RoomType::Secret,
            "public-id" => RoomType::PublicId,
            _ => RoomType::Ip,
        };

        let room_id = match room_type {
            RoomType::Ip => peer.ip.clone(),
            _ => room_id_opt.unwrap_or_default(),
        };

        if !to.is_empty() && crate::utils::is_valid_uuid(&to) {
            self.rooms.relay_signal(peer, &to, room_type, &room_id, msg);
        }
    }
}
