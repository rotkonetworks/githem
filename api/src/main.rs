mod cache;
mod metrics;
mod http;
mod ingestion;
mod websocket;

use anyhow::Result;
use std::net::SocketAddr;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "githem_api=info,tower_http=info".into()),
        )
        .init();

    let http_port = std::env::var("HTTP_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(42069);

    let http_addr = SocketAddr::from(([0, 0, 0, 0], http_port));

    let ws_port = std::env::var("WS_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(42070);

    let ws_addr = SocketAddr::from(([0, 0, 0, 0], ws_port));

    info!("Starting githem-api HTTP on http://{}", http_addr);
    info!("Starting githem-api WebSocket on ws://{}", ws_addr);

    tokio::try_join!(http::serve(http_addr), websocket::serve(ws_addr))?;

    Ok(())
}
