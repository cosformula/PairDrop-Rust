mod config;
mod peer;
mod rooms;
mod utils;
mod ws_handler;

use axum::{
    extract::{ws::WebSocketUpgrade, ConnectInfo, Query, State},
    http::{StatusCode, Uri},
    response::{IntoResponse, Redirect},
    routing::get,
    Json, Router,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::services::ServeDir;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use config::{AppConfig, ClientConfig};
use rooms::RoomManager;
use ws_handler::WsHandler;

/// Application state
#[derive(Clone)]
struct AppState {
    config: Arc<AppConfig>,
    #[allow(dead_code)]
    rooms: Arc<RoomManager>,
    ws_handler: Arc<WsHandler>,
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pairdrop_server=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Parse configuration
    let config = match AppConfig::from_cli() {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!("Configuration error: {}", e);
            std::process::exit(1);
        }
    };

    // Log configuration in debug mode
    if config.debug_mode {
        tracing::info!("DEBUG_MODE is active. To protect privacy, do not use in production.");
        tracing::debug!("----DEBUG ENVIRONMENT VARIABLES----");
        tracing::debug!("Port: {}", config.port);
        tracing::debug!("Rate limit: {:?}", config.rate_limit);
        tracing::debug!("WS Fallback: {}", config.ws_fallback);
        tracing::debug!("IPv6 Localize: {:?}", config.ipv6_localize);
        tracing::debug!("Signaling Server: {:?}", config.signaling_server);
        tracing::debug!("Localhost only: {}", config.localhost_only);
    }

    // Log IPv6 localization
    if let Some(segments) = config.ipv6_localize {
        tracing::info!(
            "IPv6 client IPs will be localized to {} {}",
            segments,
            if segments == 1 { "segment" } else { "segments" }
        );
    }

    // Log signaling server mode
    if let Some(ref server) = config.signaling_server {
        tracing::info!(
            "This instance does not include a signaling server. \
             Clients on this instance connect to the following signaling server: {}",
            server
        );
    }

    // Create room manager
    let rooms = Arc::new(RoomManager::new());

    // Create WebSocket handler
    let ws_handler = WsHandler::new(Arc::clone(&config), Arc::clone(&rooms));

    // Create app state
    let state = AppState {
        config: Arc::clone(&config),
        rooms,
        ws_handler,
    };

    // Build router
    let app = build_router(state, &config);

    // Determine bind address
    let addr = if config.localhost_only {
        SocketAddr::from(([127, 0, 0, 1], config.port))
    } else {
        SocketAddr::from(([0, 0, 0, 0], config.port))
    };

    tracing::info!("PairDrop is running on port {}", config.port);

    // Start server
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}

/// Build the application router
fn build_router(state: AppState, config: &AppConfig) -> Router {
    // Determine public directory path
    // Check for PUBLIC_DIR env var first, then try relative paths
    let public_path = std::env::var("PUBLIC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            // Try ./public first (for Docker), then ../public (for dev)
            let docker_path = PathBuf::from("./public");
            if docker_path.exists() {
                docker_path
            } else {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../public")
            }
        });

    let mut router = Router::new()
        // WebSocket endpoint
        .route("/server", get(ws_handler))
        .route("/server/", get(ws_handler))
        // Config endpoint
        .route("/config", get(config_handler))
        .route("/config/", get(config_handler));

    // Debug IP endpoint
    if config.debug_mode && config.rate_limit.is_some() {
        tracing::debug!(
            "To find out the correct value for RATE_LIMIT go to '/ip' \
             and ensure the returned IP-address is the IP-address of your client."
        );
        router = router.route("/ip", get(ip_handler));
    }

    // Static file serving
    router = router.nest_service("/", ServeDir::new(&public_path));

    // Fallback for unknown routes -> redirect to /
    router = router.fallback(fallback_handler);

    router.with_state(state)
}

/// WebSocket upgrade handler
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    // Build query string from params
    let query_string: String = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("&");

    ws.on_upgrade(move |socket| async move {
        state
            .ws_handler
            .handle_connection(socket, Some(addr), headers, query_string)
            .await
    })
}

/// Config endpoint handler
async fn config_handler(State(state): State<AppState>) -> Json<ClientConfig> {
    Json(ClientConfig {
        signaling_server: state.config.signaling_server.clone(),
        buttons: state.config.buttons.clone(),
    })
}

/// IP debug endpoint handler
async fn ip_handler(ConnectInfo(addr): ConnectInfo<SocketAddr>) -> String {
    addr.ip().to_string()
}

/// Fallback handler - redirect to /
async fn fallback_handler(uri: Uri) -> impl IntoResponse {
    // Don't redirect static assets that weren't found
    let path = uri.path();
    if path.contains('.') && !path.ends_with(".html") {
        return (StatusCode::NOT_FOUND, "Not Found").into_response();
    }

    Redirect::permanent("/").into_response()
}
