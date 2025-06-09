// websocket.rs
use anyhow::Result;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use githem_core::{IngestOptions, Ingester};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::Instant;
use tracing::{error, info};

#[derive(Debug, Deserialize)]
struct WsQuery {
    url: String,
    #[serde(default)]
    include: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
    #[serde(default = "default_max_size")]
    max_size: usize,
    #[serde(default)]
    branch: Option<String>,
}

fn default_max_size() -> usize {
    1048576
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum WsMessage {
    Progress {
        stage: String,
        message: String,
    },
    File {
        path: String,
        content: String,
    },
    Complete {
        files: usize,
        bytes: usize,
        elapsed_ms: u128,
    },
    Error {
        message: String,
    },
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WsQuery>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, params))
}

async fn handle_socket(mut socket: WebSocket, params: WsQuery) {
    let start = Instant::now();

    // Send initial progress
    if let Err(e) = socket.send(Message::Text(
        serde_json::to_string(&WsMessage::Progress {
            stage: "starting".to_string(),
            message: format!("Processing {}", params.url),
        }).unwrap().into()
    )).await {
        error!("Failed to send message: {}", e);
        return;
    }

    // Create ingester
    let options = IngestOptions {
        include_patterns: params.include,
        exclude_patterns: params.exclude,
        max_file_size: params.max_size,
        include_untracked: false,
        branch: params.branch,
    };

    // Clone repository
    if let Err(e) = socket.send(Message::Text(
        serde_json::to_string(&WsMessage::Progress {
            stage: "cloning".to_string(),
            message: "Cloning repository...".to_string(),
        }).unwrap().into()
    )).await {
        error!("Failed to send message: {}", e);
        return;
    }

    let ingester = match Ingester::from_url(&params.url, options) {
        Ok(ing) => ing,
        Err(e) => {
            let _ = socket.send(Message::Text(
                serde_json::to_string(&WsMessage::Error {
                    message: format!("Failed to clone: {}", e),
                }).unwrap().into()
            )).await;
            return;
        }
    };

    // Stream files
    if let Err(e) = socket.send(Message::Text(
        serde_json::to_string(&WsMessage::Progress {
            stage: "ingesting".to_string(),
            message: "Processing files...".to_string(),
        }).unwrap().into()
    )).await {
        error!("Failed to send message: {}", e);
        return;
    }

    // For now, collect all and send
    // TODO: Modify core to support streaming individual files
    let mut output = Vec::new();
    if let Err(e) = ingester.ingest(&mut output) {
        let _ = socket.send(Message::Text(
            serde_json::to_string(&WsMessage::Error {
                message: format!("Ingestion failed: {}", e),
            }).unwrap().into()
        )).await;
        return;
    }

    // Convert to string and send as one big file for now
    if let Ok(content) = String::from_utf8(output.clone()) {
        let _ = socket.send(Message::Text(
            serde_json::to_string(&WsMessage::File {
                path: "all_files.txt".to_string(),
                content,
            }).unwrap().into()
        )).await;
    }

    // Send completion
    let files_count = output.windows(3).filter(|w| w == b"===").count() / 2;
    let _ = socket.send(Message::Text(
        serde_json::to_string(&WsMessage::Complete {
            files: files_count,
            bytes: output.len(),
            elapsed_ms: start.elapsed().as_millis(),
        }).unwrap().into()
    )).await;

    info!("WebSocket session completed for {}", params.url);
}

pub async fn serve(addr: SocketAddr) -> Result<()> {
    let app = Router::new()
        .route("/", get(websocket_handler));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
