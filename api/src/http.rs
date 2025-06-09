// http.rs
use anyhow::Result;
use axum::{
    extract::{Path, Query},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use githem_core::{IngestOptions, Ingester};
use serde::Deserialize;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing::{info, warn};

const CLONE_TIMEOUT: Duration = Duration::from_secs(300);
const INGEST_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_RESPONSE_SIZE: usize = 50 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct QueryParams {
    #[serde(default)]
    include: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
    #[serde(default = "default_max_size")]
    max_size: usize,
    #[serde(default)]
    raw: bool,
}

fn default_max_size() -> usize {
    1048576
}

struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let msg = format!("{}", self.0);
        let status = if msg.contains("authentication") {
            StatusCode::UNAUTHORIZED
        } else if msg.contains("not found") || msg.contains("Repository") {
            StatusCode::NOT_FOUND
        } else if msg.contains("timeout") {
            StatusCode::REQUEST_TIMEOUT
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };

        warn!(error = %self.0, status = %status, "Request failed");
        (status, msg).into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

fn is_browser(headers: &HeaderMap) -> bool {
    headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| {
            s.contains("Mozilla") ||
            s.contains("Chrome") ||
            s.contains("Safari") && !s.contains("curl") && !s.contains("wget")
        })
        .unwrap_or(false)
}

fn parse_github_path(path: &str) -> Result<(String, Option<String>, Option<String>)> {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if parts.len() < 2 {
        anyhow::bail!("Invalid repository path");
    }

    let owner = parts[0];
    let repo = parts[1];
    let mut branch = None;
    let mut subpath = None;

    if parts.len() > 2 {
        match parts[2] {
            "tree" | "blob" if parts.len() > 3 => {
                branch = Some(parts[3].to_string());
                if parts.len() > 4 {
                    subpath = Some(parts[4..].join("/"));
                }
            }
            _ => {}
        }
    }

    let repo_url = format!("https://github.com/{}/{}", owner, repo);
    Ok((repo_url, branch, subpath))
}

#[axum::debug_handler]
async fn handle_request(
    Path(path): Path<String>,
    Query(params): Query<QueryParams>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let start = Instant::now();

    // If browser and no raw param, serve the frontend
    if is_browser(&headers) && !params.raw {
        return Ok(Html(format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <title>githem - {}</title>
    <meta charset="utf-8">
    <link rel="stylesheet" href="/static/style.css">
</head>
<body>
    <div id="app" data-path="{}"></div>
    <script type="module">
        window.__INITIAL_PATH__ = "{}";
        window.__WS_URL__ = "ws://localhost:3001";
    </script>
    <script src="/static/app.js"></script>
</body>
</html>"#,
            path, path, path
        )).into_response());
    }

    // Parse the GitHub-style path
    let (repo_url, branch, subpath) = parse_github_path(&path)?;

    info!(
        repo_url = %repo_url,
        branch = ?branch,
        subpath = ?subpath,
        "Processing repository"
    );

    let mut options = IngestOptions {
        include_patterns: params.include.clone(),
        exclude_patterns: params.exclude.clone(),
        max_file_size: params.max_size,
        include_untracked: false,
        branch: branch.clone(),
    };

    if let Some(ref subpath) = subpath {
        options.include_patterns.push(format!("{}/*", subpath));
    }

    let ingester = timeout(CLONE_TIMEOUT, async {
        Ingester::from_url(&repo_url, options)
    })
    .await
    .map_err(|_| anyhow::anyhow!("Repository clone timed out"))??;

    info!(elapsed_ms = start.elapsed().as_millis(), "Repository cloned");

    let mut output = Vec::new();
    timeout(INGEST_TIMEOUT, async {
        ingester.ingest(&mut output)
    })
    .await
    .map_err(|_| anyhow::anyhow!("Ingestion timed out"))??;

    if output.len() > MAX_RESPONSE_SIZE {
        return Err(anyhow::anyhow!(
            "Repository too large: {} MB (max {} MB)",
            output.len() / 1024 / 1024,
            MAX_RESPONSE_SIZE / 1024 / 1024
        ).into());
    }

    let total_elapsed = start.elapsed();
    info!(
        bytes = output.len(),
        elapsed_ms = total_elapsed.as_millis(),
        "Ingestion completed"
    );

    if params.raw || !is_browser(&headers) {
        Ok((
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            output,
        ).into_response())
    } else {
        let files_count = output
            .windows(3)
            .filter(|window| window == b"===")
            .count() / 2;

        let metadata = serde_json::json!({
            "repository": format!("{}/{}",
                path.split('/').nth(0).unwrap_or(""),
                path.split('/').nth(1).unwrap_or("")
            ),
            "branch": branch,
            "subpath": subpath,
            "files": files_count,
            "bytes": output.len(),
            "elapsed_ms": total_elapsed.as_millis(),
        });

        use base64::{Engine as _, engine::general_purpose};
        let content_b64 = general_purpose::STANDARD.encode(&output);

        let response = serde_json::json!({
            "metadata": metadata,
            "content_base64": content_b64,
        });

        Ok((StatusCode::OK, axum::Json(response)).into_response())
    }
}

async fn health() -> &'static str {
    "OK"
}

async fn index(headers: HeaderMap) -> impl IntoResponse {
    if is_browser(&headers) {
        Html(r#"<!DOCTYPE html>
<html>
<head>
    <title>githem</title>
    <meta charset="utf-8">
    <script>window.__WS_URL__ = "ws://localhost:3001";</script>
</head>
<body>
    <div id="app"></div>
    <script src="/static/app.js"></script>
</body>
</html>"#).into_response()
    } else {
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/plain")],
            "githem API - Transform git repositories into LLM-ready text\n\
            \n\
            HTTP API: GET /:owner/:repo[/tree/:branch[/:path]]\n\
            WebSocket: ws://localhost:3001\n\
            \n\
            Query parameters:\n\
            - include: Include only files matching pattern\n\
            - exclude: Exclude files matching pattern\n\
            - max_size: Maximum file size in bytes\n\
            - raw: Return plain text\n\
            \n\
            Examples:\n\
            - /gin-gonic/gin\n\
            - /rust-lang/mdBook\n"
        ).into_response()
    }
}

pub async fn serve(addr: SocketAddr) -> Result<()> {
    let app = Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .nest_service("/static", ServeDir::new("../frontend/dist"))
        .route("/*path", get(handle_request))
        .layer(
            ServiceBuilder::new()
                .layer(CorsLayer::permissive())
                .layer(CompressionLayer::new())
        );

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
