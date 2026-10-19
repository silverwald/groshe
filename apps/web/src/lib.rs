//! Web companion service for Groshe.
//!
//! Provides HTTP REST endpoints, real-time WebSocket synchronization, and embedded web assets
//! anchored to an active GrosheSyncNode peer.

pub mod api;
pub mod frontend;
pub mod server;

pub use api::*;
pub use server::{create_router, AppState};

use groshe_sync::GrosheSyncNode;
use std::net::SocketAddr;
use std::sync::Arc;

/// Starts the Groshe Web Companion HTTP & WebSocket server bound to `addr`.
pub async fn start_web_companion(
    node: Arc<GrosheSyncNode>,
    addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let state = AppState { node };
    let app = create_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Groshe Web Companion listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}