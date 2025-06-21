use crate::ingestion::{IngestionService, IngestionParams, WebSocketMessage};
use anyhow::Result;
use axum::{
    Router,
    extract::{
        Query,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use serde::Deserialize;
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
    #[serde(default)]
    preset: Option<String>,
    #[serde(default)]
    raw: bool,
}

fn default_max_size() -> usize {
    10 * 1024 * 1024
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WsQuery>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, params))
}

async fn handle_socket(mut socket: WebSocket, params: WsQuery) {
    let _start = Instant::now();

    if let Err(e) = socket
        .send(Message::Text(
            serde_json::to_string(&WebSocketMessage::Progress {
                stage: "starting".to_string(),
                message: format!("Processing {}", params.url),
            })
            .unwrap().into(),
        ))
        .await
    {
        error!("Failed to send message: {}", e);
        return;
    }

    let ingestion_params = IngestionParams {
        url: params.url.clone(),
        branch: params.branch,
        path_prefix: None,
        include_patterns: params.include,
        exclude_patterns: params.exclude,
        max_file_size: params.max_size,
        filter_preset: params.preset,
        raw: params.raw,
    };

    if let Err(e) = socket
        .send(Message::Text(
            serde_json::to_string(&WebSocketMessage::Progress {
                stage: "cloning".to_string(),
                message: "Cloning repository...".to_string(),
            })
            .unwrap().into(),
        ))
        .await
    {
        error!("Failed to send message: {}", e);
        return;
    }

    match IngestionService::ingest(ingestion_params).await {
        Ok(result) => {
            if let Err(e) = socket
                .send(Message::Text(
                    serde_json::to_string(&WebSocketMessage::Progress {
                        stage: "ingesting".to_string(),
                        message: "Processing files...".to_string(),
                    })
                    .unwrap().into(),
                ))
                .await
            {
                error!("Failed to send message: {}", e);
                return;
            }

            // Send filter stats if available
            if let Some(stats) = &result.filter_stats {
                let _ = socket
                    .send(Message::Text(
                        serde_json::to_string(&WebSocketMessage::FilterStats {
                            stats: stats.clone(),
                        })
                        .unwrap().into(),
                    ))
                    .await;
            }

            let _ = socket
                .send(Message::Text(
                    serde_json::to_string(&WebSocketMessage::File {
                        path: "all_files.txt".to_string(),
                        content: result.content,
                    })
                    .unwrap().into(),
                ))
                .await;

            let _ = socket
                .send(Message::Text(
                    serde_json::to_string(&WebSocketMessage::Complete {
                        files: result.summary.files_analyzed,
                        bytes: result.summary.total_size,
                    })
                    .unwrap().into(),
                ))
                .await;

            info!("WebSocket session completed for {}", params.url);
        }
        Err(e) => {
            let _ = socket
                .send(Message::Text(
                    serde_json::to_string(&WebSocketMessage::Error {
                        message: format!("Failed: {e}"),
                    })
                    .unwrap().into(),
                ))
                .await;
        }
    }
}

pub async fn serve(addr: SocketAddr) -> Result<()> {
    let app = Router::new().route("/", get(websocket_handler));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
