use parking_lot::RwLock;
use rand::Rng;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

use crate::peer::{Peer, PeerInfo};
use crate::utils::get_random_string;

/// Room types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RoomType {
    Ip,
    Secret,
    PublicId,
}

impl RoomType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RoomType::Ip => "ip",
            RoomType::Secret => "secret",
            RoomType::PublicId => "public-id",
        }
    }
}

/// Pair key info
struct PairKeyInfo {
    room_secret: String,
    creator: Arc<Peer>,
}

/// Room manager - handles all room operations
pub struct RoomManager {
    /// Rooms: room_id -> (peer_id -> peer)
    rooms: RwLock<HashMap<String, HashMap<String, Arc<Peer>>>>,
    /// Pair keys: pair_key -> PairKeyInfo
    pair_keys: RwLock<HashMap<String, PairKeyInfo>>,
}

impl Default for RoomManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RoomManager {
    pub fn new() -> Self {
        Self {
            rooms: RwLock::new(HashMap::new()),
            pair_keys: RwLock::new(HashMap::new()),
        }
    }

    /// Join a peer to the IP room (based on their IP)
    pub fn join_ip_room(&self, peer: &Arc<Peer>) -> Vec<PeerInfo> {
        self.join_room(peer, RoomType::Ip, &peer.ip.clone())
    }

    /// Join a peer to a secret room
    pub fn join_secret_room(&self, peer: &Arc<Peer>, room_secret: &str) -> Vec<PeerInfo> {
        peer.add_room_secret(room_secret.to_string());
        self.join_room(peer, RoomType::Secret, room_secret)
    }

    /// Join a peer to a public room
    pub fn join_public_room(&self, peer: &Arc<Peer>, room_id: &str) -> Vec<PeerInfo> {
        // Leave any existing public room first
        self.leave_public_room(peer, false);

        *peer.public_room_id.lock() = Some(room_id.to_string());
        self.join_room(peer, RoomType::PublicId, room_id)
    }

    /// Join a room and notify other peers
    /// Returns list of peers already in the room
    fn join_room(&self, peer: &Arc<Peer>, room_type: RoomType, room_id: &str) -> Vec<PeerInfo> {
        let mut rooms = self.rooms.write();

        // If peer is already in room, leave first to ensure proper notifications
        if let Some(room) = rooms.get(room_id) {
            if room.contains_key(&peer.id) {
                drop(rooms);
                self.leave_room(peer, room_type, room_id, false);
                rooms = self.rooms.write();
            }
        }

        // Create room if it doesn't exist
        let room = rooms.entry(room_id.to_string()).or_default();

        // Get existing peers before adding new one
        let existing_peers: Vec<PeerInfo> = room
            .values()
            .filter(|p| p.id != peer.id)
            .map(|p| p.get_info())
            .collect();

        // Notify existing peers about new peer
        let join_msg = serde_json::json!({
            "type": "peer-joined",
            "peer": peer.get_info(),
            "roomType": room_type.as_str(),
            "roomId": room_id
        });
        let join_msg_str = serde_json::to_string(&join_msg).unwrap();

        for other_peer in room.values() {
            if other_peer.id != peer.id {
                other_peer.send(axum::extract::ws::Message::Text(join_msg_str.clone()));
            }
        }

        // Add peer to room
        room.insert(peer.id.clone(), Arc::clone(peer));

        existing_peers
    }

    /// Leave IP room
    pub fn leave_ip_room(&self, peer: &Arc<Peer>, disconnect: bool) {
        let ip = peer.ip.clone();
        self.leave_room(peer, RoomType::Ip, &ip, disconnect);
    }

    /// Leave secret room
    pub fn leave_secret_room(&self, peer: &Arc<Peer>, room_secret: &str, disconnect: bool) {
        peer.remove_room_secret(room_secret);
        self.leave_room(peer, RoomType::Secret, room_secret, disconnect);
    }

    /// Leave public room
    pub fn leave_public_room(&self, peer: &Arc<Peer>, disconnect: bool) {
        let room_id = peer.public_room_id.lock().take();
        if let Some(room_id) = room_id {
            self.leave_room(peer, RoomType::PublicId, &room_id, disconnect);
        }
    }

    /// Leave all secret rooms
    pub fn leave_all_secret_rooms(&self, peer: &Arc<Peer>, disconnect: bool) {
        let secrets = peer.get_room_secrets();
        for secret in secrets {
            self.leave_secret_room(peer, &secret, disconnect);
        }
    }

    /// Leave a room and notify other peers
    fn leave_room(&self, peer: &Arc<Peer>, room_type: RoomType, room_id: &str, disconnect: bool) {
        let mut rooms = self.rooms.write();

        if let Some(room) = rooms.get_mut(room_id) {
            if room.remove(&peer.id).is_some() {
                // Notify remaining peers
                let leave_msg = serde_json::json!({
                    "type": "peer-left",
                    "peerId": peer.id,
                    "roomType": room_type.as_str(),
                    "roomId": room_id,
                    "disconnect": disconnect
                });
                let leave_msg_str = serde_json::to_string(&leave_msg).unwrap();

                for other_peer in room.values() {
                    other_peer.send(axum::extract::ws::Message::Text(leave_msg_str.clone()));
                }

                // Remove room if empty
                if room.is_empty() {
                    rooms.remove(room_id);
                }
            }
        }
    }

    /// Initiate device pairing - returns (room_secret, pair_key)
    pub fn pair_device_initiate(&self, peer: &Arc<Peer>) -> (String, String) {
        let room_secret = get_random_string(256, false);
        let pair_key = self.create_pair_key(peer, &room_secret);

        // Remove old pair key if exists
        if let Some(old_key) = peer.pair_key.lock().take() {
            self.remove_pair_key(&old_key);
        }

        *peer.pair_key.lock() = Some(pair_key.clone());

        // Join secret room
        self.join_secret_room(peer, &room_secret);

        (room_secret, pair_key)
    }

    /// Join device pairing - returns (room_secret, creator_peer_id) or None if invalid
    pub fn pair_device_join(&self, peer: &Arc<Peer>, pair_key: &str) -> Option<(String, String)> {
        let mut pair_keys = self.pair_keys.write();

        if let Some(info) = pair_keys.remove(pair_key) {
            // Don't allow joining own pair key
            if info.creator.id == peer.id {
                // Put it back
                pair_keys.insert(
                    pair_key.to_string(),
                    PairKeyInfo {
                        room_secret: info.room_secret,
                        creator: info.creator,
                    },
                );
                return None;
            }

            let room_secret = info.room_secret.clone();
            let creator_id = info.creator.id.clone();

            // Clear creator's pair key
            *info.creator.pair_key.lock() = None;

            // Join joiner to secret room
            drop(pair_keys);
            self.join_secret_room(peer, &room_secret);

            // Clear joiner's pair key if any
            if let Some(joiner_key) = peer.pair_key.lock().take() {
                self.remove_pair_key(&joiner_key);
            }

            Some((room_secret, creator_id))
        } else {
            None
        }
    }

    /// Cancel device pairing
    pub fn pair_device_cancel(&self, peer: &Arc<Peer>) -> Option<String> {
        let pair_key = peer.pair_key.lock().take();
        if let Some(ref key) = pair_key {
            self.remove_pair_key(key);
        }
        pair_key
    }

    /// Create a new pair key
    fn create_pair_key(&self, creator: &Arc<Peer>, room_secret: &str) -> String {
        let mut pair_keys = self.pair_keys.write();
        let mut rng = rand::thread_rng();

        loop {
            // Generate 6-digit code (100000-999999)
            let key: u32 = rng.gen_range(100000..1000000);
            let key_str = key.to_string();

            if !pair_keys.contains_key(&key_str) {
                pair_keys.insert(
                    key_str.clone(),
                    PairKeyInfo {
                        room_secret: room_secret.to_string(),
                        creator: Arc::clone(creator),
                    },
                );
                return key_str;
            }
        }
    }

    /// Remove a pair key
    pub fn remove_pair_key(&self, pair_key: &str) {
        let mut pair_keys = self.pair_keys.write();
        if let Some(info) = pair_keys.remove(pair_key) {
            *info.creator.pair_key.lock() = None;
        }
    }

    /// Create a public room
    pub fn create_public_room(&self, peer: &Arc<Peer>) -> String {
        let room_id = get_random_string(5, true).to_lowercase();
        self.join_public_room(peer, &room_id);
        room_id
    }

    /// Check if public room exists
    pub fn public_room_exists(&self, room_id: &str) -> bool {
        self.rooms.read().contains_key(room_id)
    }

    /// Regenerate room secret
    pub fn regenerate_room_secret(&self, old_secret: &str) -> String {
        let new_secret = get_random_string(256, false);

        let mut rooms = self.rooms.write();

        if let Some(room) = rooms.remove(old_secret) {
            // Notify all peers in the room
            let msg = serde_json::json!({
                "type": "room-secret-regenerated",
                "oldRoomSecret": old_secret,
                "newRoomSecret": new_secret
            });
            let msg_str = serde_json::to_string(&msg).unwrap();

            for peer in room.values() {
                peer.remove_room_secret(old_secret);
                peer.send(axum::extract::ws::Message::Text(msg_str.clone()));
            }
        }

        new_secret
    }

    /// Delete a secret room
    pub fn delete_secret_room(&self, room_secret: &str) {
        let mut rooms = self.rooms.write();

        if let Some(room) = rooms.remove(room_secret) {
            let msg = serde_json::json!({
                "type": "secret-room-deleted",
                "roomSecret": room_secret
            });
            let msg_str = serde_json::to_string(&msg).unwrap();

            for peer in room.values() {
                peer.remove_room_secret(room_secret);
                peer.send(axum::extract::ws::Message::Text(msg_str.clone()));
            }
        }
    }

    /// Relay a signal message to a specific peer
    pub fn relay_signal(
        &self,
        sender: &Arc<Peer>,
        to_peer_id: &str,
        _room_type: RoomType,
        room_id: &str,
        mut message: serde_json::Value,
    ) -> bool {
        let rooms = self.rooms.read();

        if let Some(room) = rooms.get(room_id) {
            if let Some(recipient) = room.get(to_peer_id) {
                // Remove 'to' field and add sender info
                if let Some(obj) = message.as_object_mut() {
                    obj.remove("to");
                    obj.insert(
                        "sender".to_string(),
                        serde_json::json!({
                            "id": sender.id,
                            "rtcSupported": sender.rtc_supported
                        }),
                    );
                }

                let msg_str = serde_json::to_string(&message).unwrap();
                return recipient.send(axum::extract::ws::Message::Text(msg_str));
            }
        }

        false
    }

    /// Get a peer by ID from any room
    pub fn get_peer(&self, peer_id: &str) -> Option<Arc<Peer>> {
        let rooms = self.rooms.read();
        for room in rooms.values() {
            if let Some(peer) = room.get(peer_id) {
                return Some(Arc::clone(peer));
            }
        }
        None
    }
}
