use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// PairDrop Server Configuration
#[derive(Parser, Debug, Clone)]
#[command(name = "pairdrop-server")]
#[command(about = "PairDrop signaling server")]
pub struct Config {
    /// Server port
    #[arg(short, long, env = "PORT", default_value = "3000")]
    pub port: u16,

    /// Enable debug mode
    #[arg(long, env = "DEBUG_MODE", default_value = "false")]
    pub debug_mode: bool,

    /// Rate limit (number of proxies to trust, or false to disable)
    #[arg(long, env = "RATE_LIMIT")]
    pub rate_limit: Option<u8>,

    /// Enable rate limiting with default value (5)
    #[arg(long)]
    pub rate_limit_flag: bool,

    /// Enable WebSocket fallback when WebRTC unavailable
    #[arg(long, env = "WS_FALLBACK", default_value = "false")]
    pub ws_fallback: bool,

    /// Also accept --include-ws-fallback for compatibility
    #[arg(long = "include-ws-fallback")]
    pub include_ws_fallback: bool,

    /// Path to RTC config JSON file
    #[arg(long, env = "RTC_CONFIG")]
    pub rtc_config: Option<PathBuf>,

    /// External signaling server URL (without protocol)
    #[arg(long, env = "SIGNALING_SERVER")]
    pub signaling_server: Option<String>,

    /// IPv6 localization segments (1-7)
    #[arg(long, env = "IPV6_LOCALIZE")]
    pub ipv6_localize: Option<u8>,

    /// Only listen on localhost
    #[arg(long)]
    pub localhost_only: bool,

    /// Auto-restart on error (not implemented in Rust version)
    #[arg(long)]
    pub auto_restart: bool,
}

/// Button configuration for About page
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ButtonConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// All button configurations
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ButtonsConfig {
    pub donation_button: ButtonConfig,
    pub twitter_button: ButtonConfig,
    pub mastodon_button: ButtonConfig,
    pub bluesky_button: ButtonConfig,
    pub custom_button: ButtonConfig,
    pub privacypolicy_button: ButtonConfig,
}

/// WebRTC configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RtcConfig {
    pub sdp_semantics: String,
    pub ice_servers: Vec<IceServer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IceServer {
    pub urls: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

impl Default for RtcConfig {
    fn default() -> Self {
        RtcConfig {
            sdp_semantics: "unified-plan".to_string(),
            ice_servers: vec![IceServer {
                urls: "stun:stun.l.google.com:19302".to_string(),
                username: None,
                credential: None,
            }],
        }
    }
}

/// Runtime configuration after parsing
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub port: u16,
    pub debug_mode: bool,
    pub rate_limit: Option<u8>,
    pub ws_fallback: bool,
    pub rtc_config: RtcConfig,
    pub signaling_server: Option<String>,
    pub ipv6_localize: Option<u8>,
    pub localhost_only: bool,
    pub buttons: ButtonsConfig,
}

impl AppConfig {
    pub fn from_cli() -> Result<Self, String> {
        let cli = Config::parse();

        // Determine rate limit
        let rate_limit = if cli.rate_limit_flag {
            Some(5)
        } else {
            cli.rate_limit
        };

        // Determine ws_fallback
        let ws_fallback = cli.ws_fallback || cli.include_ws_fallback;

        // Load RTC config
        let rtc_config = if let Some(path) = &cli.rtc_config {
            let content = fs::read_to_string(path)
                .map_err(|e| format!("Failed to read RTC config file: {}", e))?;
            serde_json::from_str(&content)
                .map_err(|e| format!("Failed to parse RTC config JSON: {}", e))?
        } else {
            RtcConfig::default()
        };

        // Validate signaling server
        let signaling_server = if let Some(mut server) = cli.signaling_server {
            // Validate URL format
            if server.contains("://") {
                return Err(
                    "SIGNALING_SERVER must be a valid URL without the protocol prefix".to_string(),
                );
            }

            // Ensure trailing slash
            if !server.ends_with('/') {
                server.push('/');
            }

            // Check for incompatible options
            if cli.rtc_config.is_some() || ws_fallback || cli.ipv6_localize.is_some() {
                return Err("SIGNALING_SERVER cannot be used alongside WS_FALLBACK, RTC_CONFIG or IPV6_LOCALIZE".to_string());
            }

            Some(server)
        } else {
            None
        };

        // Validate IPv6 localize
        if let Some(v) = cli.ipv6_localize {
            if !(1..=7).contains(&v) {
                return Err("IPV6_LOCALIZE must be an integer between 1 and 7".to_string());
            }
        }

        // Load button configuration from environment
        let buttons = ButtonsConfig {
            donation_button: ButtonConfig {
                active: std::env::var("DONATION_BUTTON_ACTIVE").ok().map(|v| v == "true"),
                link: std::env::var("DONATION_BUTTON_LINK").ok(),
                title: std::env::var("DONATION_BUTTON_TITLE").ok(),
            },
            twitter_button: ButtonConfig {
                active: std::env::var("TWITTER_BUTTON_ACTIVE").ok().map(|v| v == "true"),
                link: std::env::var("TWITTER_BUTTON_LINK").ok(),
                title: std::env::var("TWITTER_BUTTON_TITLE").ok(),
            },
            mastodon_button: ButtonConfig {
                active: std::env::var("MASTODON_BUTTON_ACTIVE").ok().map(|v| v == "true"),
                link: std::env::var("MASTODON_BUTTON_LINK").ok(),
                title: std::env::var("MASTODON_BUTTON_TITLE").ok(),
            },
            bluesky_button: ButtonConfig {
                active: std::env::var("BLUESKY_BUTTON_ACTIVE").ok().map(|v| v == "true"),
                link: std::env::var("BLUESKY_BUTTON_LINK").ok(),
                title: std::env::var("BLUESKY_BUTTON_TITLE").ok(),
            },
            custom_button: ButtonConfig {
                active: std::env::var("CUSTOM_BUTTON_ACTIVE").ok().map(|v| v == "true"),
                link: std::env::var("CUSTOM_BUTTON_LINK").ok(),
                title: std::env::var("CUSTOM_BUTTON_TITLE").ok(),
            },
            privacypolicy_button: ButtonConfig {
                active: std::env::var("PRIVACYPOLICY_BUTTON_ACTIVE").ok().map(|v| v == "true"),
                link: std::env::var("PRIVACYPOLICY_BUTTON_LINK").ok(),
                title: std::env::var("PRIVACYPOLICY_BUTTON_TITLE").ok(),
            },
        };

        Ok(AppConfig {
            port: cli.port,
            debug_mode: cli.debug_mode,
            rate_limit,
            ws_fallback,
            rtc_config,
            signaling_server,
            ipv6_localize: cli.ipv6_localize,
            localhost_only: cli.localhost_only,
            buttons,
        })
    }
}

/// Configuration sent to WebSocket clients
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WsConfig {
    pub rtc_config: RtcConfig,
    pub ws_fallback: bool,
}

/// Configuration sent via /config endpoint
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientConfig {
    pub signaling_server: Option<String>,
    pub buttons: ButtonsConfig,
}
