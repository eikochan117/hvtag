pub mod error;
pub mod routes;
pub mod state;

use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use tracing::{info, warn};

use crate::config::Config;
use state::AppState;

/// Launches the local web UI server. Owns the `Connection` for the remainder of the process
/// (this branch runs exclusively and never returns until shutdown), wrapping it for shared
/// access across concurrent handlers.
///
/// `bind_override` lets `--ui-bind` override `config.toml`'s `[ui]` bind address/port for a
/// single run without editing the file — accepts either a bare host (keeps the configured port)
/// or a full `host:port` string.
pub async fn run_ui_workflow(
    db: Connection,
    config: &Config,
    bind_override: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = AppState {
        db: Arc::new(Mutex::new(db)),
        page_size: config.ui.page_size,
    };
    let app = routes::build_router(state);

    let addr_str = match bind_override {
        Some(ref b) if b.contains(':') => b.clone(),
        Some(ref b) => format!("{}:{}", b, config.ui.port),
        None => format!("{}:{}", config.ui.bind_address, config.ui.port),
    };
    let addr: std::net::SocketAddr = addr_str.parse()
        .map_err(|e| format!("Invalid bind address '{}': {}", addr_str, e))?;

    if !addr.ip().is_loopback() {
        warn!(
            "hvtag web UI is binding to {} (not loopback). This is only safe if reachable \
             exclusively via your VPN/Tailscale boundary — there is no authentication layer \
             in this version.",
            addr.ip()
        );
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("hvtag web UI listening on http://{}", addr);
    info!("  Works:   http://{}/works", addr);
    info!("  Tags:    http://{}/tags", addr);
    info!("  Circles: http://{}/circles", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    info!("Shutting down hvtag web UI...");
}
