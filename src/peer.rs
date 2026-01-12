use axum::extract::ws::Message;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use uuid::Uuid;
use woothee::parser::Parser;

use crate::utils::{cyrb53, hash_code_salted, is_valid_uuid};

/// Color names for display name generation
const COLORS: &[&str] = &[
    "Red", "Orange", "Yellow", "Green", "Blue", "Purple", "Pink", "Brown", "Gray", "Black",
    "White", "Cyan", "Magenta", "Lime", "Teal", "Indigo", "Violet", "Coral", "Salmon", "Crimson",
    "Maroon", "Navy", "Olive", "Silver", "Gold", "Bronze", "Copper", "Amber", "Jade", "Ruby",
    "Emerald", "Sapphire", "Topaz", "Pearl", "Ivory", "Ebony", "Scarlet", "Azure", "Beige", "Tan",
];

/// Animal names for display name generation
const ANIMALS: &[&str] = &[
    "Dog", "Cat", "Bird", "Fish", "Lion", "Tiger", "Bear", "Wolf", "Fox", "Deer", "Rabbit",
    "Mouse", "Horse", "Cow", "Pig", "Sheep", "Goat", "Chicken", "Duck", "Eagle", "Hawk", "Owl",
    "Parrot", "Penguin", "Dolphin", "Whale", "Shark", "Turtle", "Frog", "Snake", "Lizard",
    "Crocodile", "Elephant", "Giraffe", "Zebra", "Monkey", "Gorilla", "Panda", "Koala", "Kangaroo",
    "Beaver", "Otter", "Seal", "Walrus", "Moose", "Elk", "Buffalo", "Rhino", "Hippo", "Camel",
];

/// Peer name information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerName {
    pub model: Option<String>,
    pub os: Option<String>,
    pub browser: Option<String>,
    #[serde(rename = "type")]
    pub device_type: Option<String>,
    pub device_name: String,
    pub display_name: String,
}

/// Peer info sent to other peers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerInfo {
    pub id: String,
    pub name: PeerName,
    pub rtc_supported: bool,
}

/// Rate limiter for a peer
#[derive(Debug)]
pub struct RateLimiter {
    requests: Vec<Instant>,
    max_requests: usize,
    window: Duration,
}

impl RateLimiter {
    pub fn new(max_requests: usize, window: Duration) -> Self {
        Self {
            requests: Vec::new(),
            max_requests,
            window,
        }
    }

    pub fn check(&mut self) -> bool {
        let now = Instant::now();
        // Remove old requests
        self.requests.retain(|&t| now.duration_since(t) < self.window);

        if self.requests.len() >= self.max_requests {
            return false;
        }

        self.requests.push(now);
        true
    }
}

/// Represents a connected peer
pub struct Peer {
    pub id: String,
    pub ip: String,
    pub name: PeerName,
    pub rtc_supported: bool,
    pub room_secrets: Mutex<HashSet<String>>,
    pub pair_key: Mutex<Option<String>>,
    pub public_room_id: Mutex<Option<String>>,
    pub sender: mpsc::UnboundedSender<Message>,
    rate_limiter: Mutex<RateLimiter>,
}

impl Peer {
    /// Create a new peer from connection info
    pub fn new(
        sender: mpsc::UnboundedSender<Message>,
        remote_addr: Option<IpAddr>,
        headers: &axum::http::HeaderMap,
        query_params: &std::collections::HashMap<String, String>,
        ipv6_localize: Option<u8>,
        debug_mode: bool,
    ) -> Arc<Self> {
        // Extract IP
        let ip = Self::extract_ip(remote_addr, headers, ipv6_localize, debug_mode);

        // Extract peer ID (reuse if valid)
        let id = Self::extract_peer_id(query_params);

        // Check WebRTC support
        let rtc_supported = query_params
            .get("webrtc_supported")
            .map(|v| v == "true")
            .unwrap_or(false);

        // Parse user agent and generate name
        let user_agent = headers
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let name = Self::generate_name(user_agent, &id);

        Arc::new(Self {
            id,
            ip,
            name,
            rtc_supported,
            room_secrets: Mutex::new(HashSet::new()),
            pair_key: Mutex::new(None),
            public_room_id: Mutex::new(None),
            sender,
            rate_limiter: Mutex::new(RateLimiter::new(10, Duration::from_secs(10))),
        })
    }

    /// Extract client IP from headers or socket
    fn extract_ip(
        remote_addr: Option<IpAddr>,
        headers: &axum::http::HeaderMap,
        ipv6_localize: Option<u8>,
        debug_mode: bool,
    ) -> String {
        // Try cf-connecting-ip first (Cloudflare)
        let mut ip = headers
            .get("cf-connecting-ip")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.split(',').next().unwrap_or("").trim().to_string());

        // Then x-forwarded-for
        if ip.is_none() {
            ip = headers
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.split(',').next().unwrap_or("").trim().to_string());
        }

        // Finally socket remote address
        if ip.is_none() {
            ip = remote_addr.map(|addr| addr.to_string());
        }

        let mut ip = ip.unwrap_or_default();

        // Remove IPv4-mapped IPv6 prefix
        if ip.starts_with("::ffff:") {
            ip = ip[7..].to_string();
        }

        let mut ipv6_was_localized = false;

        // IPv6 localization
        if let Some(segments) = ipv6_localize {
            if ip.contains(':') {
                let parts: Vec<&str> = ip.split(':').collect();
                if parts.len() > segments as usize {
                    ip = parts[..segments as usize].join(":");
                    ipv6_was_localized = true;
                }
            }
        }

        if debug_mode {
            tracing::debug!("----DEBUGGING-PEER-IP-START----");
            tracing::debug!("remoteAddress: {:?}", remote_addr);
            tracing::debug!(
                "x-forwarded-for: {:?}",
                headers.get("x-forwarded-for").and_then(|v| v.to_str().ok())
            );
            tracing::debug!(
                "cf-connecting-ip: {:?}",
                headers
                    .get("cf-connecting-ip")
                    .and_then(|v| v.to_str().ok())
            );
            if ipv6_was_localized {
                tracing::debug!(
                    "IPv6 client IP was localized to {} segments",
                    ipv6_localize.unwrap()
                );
            }
            tracing::debug!("PairDrop uses: {}", ip);
            tracing::debug!("IP is private: {}", Self::is_private_ip(&ip));
            tracing::debug!("if IP is private, '127.0.0.1' is used instead");
            tracing::debug!("----DEBUGGING-PEER-IP-END----");
        }

        // Treat localhost and private IPs as 127.0.0.1
        if ip == "::1" || Self::is_private_ip(&ip) {
            ip = "127.0.0.1".to_string();
        }

        ip
    }

    /// Check if IP is private
    fn is_private_ip(ip: &str) -> bool {
        // IPv4 private ranges
        if !ip.contains(':') {
            let parts: Vec<u8> = ip
                .split('.')
                .filter_map(|s| s.parse().ok())
                .collect();

            if parts.len() == 4 {
                // 10.0.0.0/8
                if parts[0] == 10 {
                    return true;
                }
                // 172.16.0.0/12
                if parts[0] == 172 && (16..=31).contains(&parts[1]) {
                    return true;
                }
                // 192.168.0.0/16
                if parts[0] == 192 && parts[1] == 168 {
                    return true;
                }
            }
            return false;
        }

        // IPv6 private ranges
        let first_word = ip.split(':').find(|s| !s.is_empty());
        if let Some(word) = first_word {
            let word_lower = word.to_lowercase();

            // Site local (deprecated): fec0::/10
            if word_lower.starts_with("fec")
                || word_lower.starts_with("fed")
                || word_lower.starts_with("fee")
                || word_lower.starts_with("fef")
            {
                return true;
            }

            // Unique Local Address: fc00::/7
            if word_lower.starts_with("fc") || word_lower.starts_with("fd") {
                return true;
            }

            // Link local: fe80::/10
            if word_lower == "fe80" {
                return true;
            }

            // Discard prefix
            if word == "100" {
                return true;
            }
        }

        false
    }

    /// Extract or generate peer ID
    fn extract_peer_id(query_params: &std::collections::HashMap<String, String>) -> String {
        if let (Some(peer_id), Some(peer_id_hash)) =
            (query_params.get("peer_id"), query_params.get("peer_id_hash"))
        {
            if is_valid_uuid(peer_id) && Self::is_peer_id_hash_valid(peer_id, peer_id_hash) {
                return peer_id.clone();
            }
        }
        Uuid::new_v4().to_string()
    }

    /// Validate peer ID hash
    fn is_peer_id_hash_valid(peer_id: &str, peer_id_hash: &str) -> bool {
        hash_code_salted(peer_id) == peer_id_hash
    }

    /// Generate peer name from user agent
    fn generate_name(user_agent: &str, peer_id: &str) -> PeerName {
        let parser = Parser::new();
        let result = parser.parse(user_agent);

        let (os, browser, device_type, model) = if let Some(r) = result {
            (
                Some(r.os.to_string()),
                Some(r.name.to_string()),
                if r.category == "smartphone" || r.category == "mobilephone" {
                    Some("mobile".to_string())
                } else if r.category == "pc" {
                    Some("desktop".to_string())
                } else {
                    Some(r.category.to_string())
                },
                None, // woothee doesn't provide device model
            )
        } else {
            (None, None, None, None)
        };

        // Build device name
        let mut device_name = String::new();
        if let Some(ref os_name) = os {
            let os_short = os_name.replace("Mac OS X", "Mac");
            device_name.push_str(&os_short);
            device_name.push(' ');
        }
        if let Some(ref browser_name) = browser {
            device_name.push_str(browser_name);
        }
        if device_name.is_empty() {
            device_name = "Unknown Device".to_string();
        }

        // Generate display name from peer ID
        let hash = cyrb53(peer_id, 0);
        let color_idx = (hash % COLORS.len() as u64) as usize;
        let animal_idx = ((hash / COLORS.len() as u64) % ANIMALS.len() as u64) as usize;
        let display_name = format!("{} {}", COLORS[color_idx], ANIMALS[animal_idx]);

        PeerName {
            model,
            os,
            browser,
            device_type,
            device_name,
            display_name,
        }
    }

    /// Get peer info for sending to other peers
    pub fn get_info(&self) -> PeerInfo {
        PeerInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            rtc_supported: self.rtc_supported,
        }
    }

    /// Check rate limit
    pub fn rate_limit_reached(&self) -> bool {
        !self.rate_limiter.lock().check()
    }

    /// Add a room secret
    pub fn add_room_secret(&self, secret: String) {
        self.room_secrets.lock().insert(secret);
    }

    /// Remove a room secret
    pub fn remove_room_secret(&self, secret: &str) {
        self.room_secrets.lock().remove(secret);
    }

    /// Get all room secrets
    pub fn get_room_secrets(&self) -> Vec<String> {
        self.room_secrets.lock().iter().cloned().collect()
    }

    /// Send a message to this peer
    pub fn send(&self, msg: Message) -> bool {
        self.sender.send(msg).is_ok()
    }

    /// Send a JSON message to this peer
    pub fn send_json<T: Serialize>(&self, data: &T) -> bool {
        match serde_json::to_string(data) {
            Ok(json) => self.send(Message::Text(json)),
            Err(e) => {
                tracing::error!("Failed to serialize message: {}", e);
                false
            }
        }
    }

    /// Get peer ID hash for authentication
    pub fn get_peer_id_hash(&self) -> String {
        hash_code_salted(&self.id)
    }
}
