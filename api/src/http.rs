use crate::ingestion::{IngestionParams, IngestionResult, IngestionService};
use githem_core::validate_github_name;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use tokio::time::timeout;
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer, cors::CorsLayer, set_header::SetResponseHeaderLayer,
};

const INGEST_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone)]
pub struct AppState {
    pub cache: Arc<tokio::sync::RwLock<HashMap<String, CachedResult>>>,
}

#[derive(Clone, Debug)]
pub struct CachedResult {
    pub result: IngestionResult,
    pub created_at: Instant,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IngestRequest {
    pub url: String,
    pub branch: Option<String>,
    pub subpath: Option<String>,
    pub path_prefix: Option<String>,
    #[serde(default)]
    pub include_patterns: Vec<String>,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    #[serde(default = "default_max_file_size")]
    pub max_file_size: usize,
    /// Filter preset: "raw", "standard", "code-only", "minimal"
    pub filter_preset: Option<String>,
    /// Raw mode - disable all filtering
    #[serde(default)]
    pub raw: bool,
}

fn default_max_file_size() -> usize {
    10 * 1024 * 1024
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IngestResponse {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: String,
}

#[derive(Debug)]
pub enum AppError {
    InvalidRequest(String),
    NotFound,
    Timeout,
    InternalError(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, error_response) = match self {
            AppError::InvalidRequest(msg) => (
                StatusCode::BAD_REQUEST,
                ErrorResponse {
                    error: msg,
                    code: "INVALID_REQUEST".to_string(),
                },
            ),
            AppError::NotFound => (
                StatusCode::NOT_FOUND,
                ErrorResponse {
                    error: "Resource not found".to_string(),
                    code: "NOT_FOUND".to_string(),
                },
            ),
            AppError::Timeout => (
                StatusCode::REQUEST_TIMEOUT,
                ErrorResponse {
                    error: "Request timed out".to_string(),
                    code: "TIMEOUT".to_string(),
                },
            ),
            AppError::InternalError(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorResponse {
                    error: msg,
                    code: "INTERNAL_ERROR".to_string(),
                },
            ),
        };

        (status, Json(error_response)).into_response()
    }
}

#[derive(Deserialize)]
pub struct QueryParams {
    pub branch: Option<String>,
    pub subpath: Option<String>,
    pub include: Option<String>,
    pub exclude: Option<String>,
    pub max_size: Option<usize>,
    pub preset: Option<String>,
    pub raw: Option<bool>,
    pub path: Option<String>,
}

async fn api_info() -> impl IntoResponse {
    Json(serde_json::json!({
        "name": "Githem API",
        "description": "Powertool for grabbing git repositories to be fed for LLMs",
        "version": env!("CARGO_PKG_VERSION"),
        "endpoints": {
            "GET /": {
                "description": "API information and usage documentation",
                "response": "This endpoint - API documentation"
            },
            "GET /health": {
                "description": "Health check endpoint",
                "response": "Service status, timestamp, and version"
            },
            "POST /api/ingest": {
                "description": "Ingest a repository from any Git URL",
                "request_body": {
                    "url": "Git repository URL (required)",
                    "branch": "Branch name (optional, defaults to default branch)",
                    "subpath": "Subpath within repository (optional)",
                    "path_prefix": "Path prefix (optional, alias for subpath)",
                    "include_patterns": "Array of file patterns to include (optional)",
                    "exclude_patterns": "Array of file patterns to exclude (optional)",
                    "max_file_size": "Maximum file size in bytes (optional, default: 10MB)",
                    "filter_preset": "Filter preset: raw, standard, code-only, minimal (optional, default: standard)",
                    "raw": "Raw mode - disable all filtering (optional, default: false)"
                },
                "response": "Ingestion ID and status",
                "example": {
                    "url": "https://github.com/owner/repo",
                    "branch": "main",
                    "include_patterns": ["*.rs", "*.md"],
                    "exclude_patterns": ["target/", "*.lock"],
                    "filter_preset": "standard"
                }
            },
            "GET /api/result/{id}": {
                "description": "Get ingestion result by ID",
                "parameters": {
                    "id": "Ingestion ID from /api/ingest response"
                },
                "response": "Complete ingestion result with metadata and content"
            },
            "GET /api/download/{id}": {
                "description": "Download ingested content as text file",
                "parameters": {
                    "id": "Ingestion ID from /api/ingest response"
                },
                "response": "Text file download with repository content"
            },
            "GET /{owner}/{repo}": {
                "description": "Direct GitHub repository ingestion",
                "parameters": {
                    "owner": "GitHub repository owner",
                    "repo": "GitHub repository name"
                },
                "query_parameters": {
                    "branch": "Branch name (optional)",
                    "subpath": "Subpath within repository (optional)",
                    "include": "Comma-separated include patterns (optional)",
                    "exclude": "Comma-separated exclude patterns (optional)",
                    "max_size": "Maximum file size in bytes (optional)",
                    "preset": "Filter preset: raw, standard, code-only, minimal (optional, default: standard)",
                    "raw": "Raw mode - disable filtering (optional)"
                },
                "response": "Repository content as plain text",
                "example": "/microsoft/typescript?branch=main&include=*.ts,*.md&exclude=node_modules&preset=code-only"
            },
            "GET /{owner}/{repo}/tree/{branch}": {
                "description": "GitHub repository ingestion with specific branch",
                "parameters": {
                    "owner": "GitHub repository owner",
                    "repo": "GitHub repository name",
                    "branch": "Branch name"
                },
                "query_parameters": "Same as /{owner}/{repo}",
                "response": "Repository content as plain text"
            },
            "GET /{owner}/{repo}/tree/{branch}/*path": {
                "description": "GitHub repository ingestion with specific branch and path",
                "parameters": {
                    "owner": "GitHub repository owner",
                    "repo": "GitHub repository name",
                    "branch": "Branch name",
                    "path": "Path within repository"
                },
                "query_parameters": "Same as /{owner}/{repo}",
                "response": "Repository content as plain text"
            },
            "GET /{owner}/{repo}/compare/{compare_spec}": {
                "description": "Compare two branches/commits and get the diff",
                "parameters": {
                    "owner": "GitHub repository owner",
                    "repo": "GitHub repository name",
                    "compare_spec": "Comparison specification (e.g., 'main...feature' or 'main..feature')"
                },
                "query_parameters": {
                    "preset": "Filter preset for diff output (optional)",
                    "include": "Include only files matching patterns in diff (optional)",
                    "exclude": "Exclude files matching patterns from diff (optional)"
                },
                "response": "Unified diff format with statistics",
                "example": "/polytope-labs/hyperbridge/compare/main...tendermint"
            }
        },
        "usage_examples": {
            "curl_examples": [
                {
                    "description": "Ingest a repository with standard filtering",
                    "command": "curl -X POST http://localhost:42069/api/ingest -H \"Content-Type: application/json\" -d '{\"url\": \"https://github.com/owner/repo\", \"branch\": \"main\", \"filter_preset\": \"standard\"}'"
                },
                {
                    "description": "Ingest with raw mode (no filtering)",
                    "command": "curl -X POST http://localhost:42069/api/ingest -H \"Content-Type: application/json\" -d '{\"url\": \"https://github.com/owner/repo\", \"raw\": true}'"
                },
                {
                    "description": "Get ingestion result",
                    "command": "curl http://localhost:42069/api/result/{id}"
                },
                {
                    "description": "Download content",
                    "command": "curl http://localhost:42069/api/download/{id}"
                },
                {
                    "description": "Direct GitHub ingestion with code-only preset",
                    "command": "curl http://localhost:42069/microsoft/typescript?branch=main&preset=code-only"
                },
                {
                    "description": "Compare branches",
                    "command": "curl http://localhost:42069/owner/repo/compare/main...feature"
                }
            ]
        },
        "filtering": {
            "presets": {
                "raw": "No filtering - include everything",
                "standard": "Smart filtering for LLM analysis (default)",
                "code-only": "Only source code files, exclude documentation",
                "minimal": "Basic filtering - exclude obvious binary/large files"
            },
            "default_excludes": [
                "Lock files: *.lock, package-lock.json, Cargo.lock",
                "Dependencies: node_modules/, target/, vendor/",
                "Build artifacts: dist/, build/, .next/",
                "Media files: images, videos, fonts",
                "Binary files: archives, executables",
                "IDE files: .vscode/, .idea/, .DS_Store"
            ]
        },
        "notes": {
            "timeout": "Ingestion requests timeout after 300 seconds",
            "cache": "Results are cached in memory (max 100 entries, LRU eviction)",
            "file_size": "Default maximum file size is 10MB",
            "github_shortcut": "GitHub URLs can be accessed directly via /{owner}/{repo} paths",
            "filtering": "Smart filtering is enabled by default. Use raw=true or preset=raw to disable.",
            "compare": "Compare supports both two-dot (..) and three-dot (...) syntax"
        }
    }))
}

async fn health() -> impl IntoResponse {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    Json(serde_json::json!({
        "status": "ok",
        "timestamp": timestamp,
        "version": env!("CARGO_PKG_VERSION")
    }))
}

async fn ingest_repository(
    State(state): State<AppState>,
    Json(request): Json<IngestRequest>,
) -> Result<impl IntoResponse, AppError> {
    let params = IngestionParams {
        url: request.url,
        subpath: request.subpath.clone(),
        branch: request.branch,
        path_prefix: request.path_prefix.or(request.subpath),
        include_patterns: request.include_patterns,
        exclude_patterns: request.exclude_patterns,
        max_file_size: request.max_file_size,
        filter_preset: request.filter_preset,
        raw: request.raw,
    };

    let ingestion_result = timeout(INGEST_TIMEOUT, async {
        IngestionService::ingest(params).await
    })
    .await
    .map_err(|_| AppError::Timeout)?
    .map_err(|e| AppError::InternalError(format!("Ingestion failed: {}", e)))?;

    let cached = CachedResult {
        result: ingestion_result.clone(),
        created_at: Instant::now(),
    };

    {
        let mut cache = state.cache.write().await;
        cache.insert(ingestion_result.id.clone(), cached);

        if cache.len() > 100 {
            let mut entries: Vec<_> = cache
                .iter()
                .map(|(k, v)| (k.clone(), v.created_at))
                .collect();
            entries.sort_by_key(|(_, time)| *time);
            for (key, _) in entries.iter().take(cache.len() - 100) {
                cache.remove(key);
            }
        }
    }

    Ok(Json(IngestResponse {
        id: ingestion_result.id.clone(),
        status: "completed".to_string(),
    }))
}

async fn get_result(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let cache = state.cache.read().await;

    match cache.get(&id) {
        Some(cached) => {
            let result = cached.result.clone();
            drop(cache);
            Ok(Json(result))
        }
        None => Err(AppError::NotFound),
    }
}

async fn download_content(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let cache = state.cache.read().await;

    match cache.get(&id) {
        Some(cached) => {
            let content = cached.result.content.clone();
            let filename = format!("githem-{id}.txt");
            drop(cache);

            let mut headers = HeaderMap::new();
            headers.insert(
                "content-type",
                "text/plain; charset=utf-8"
                    .parse()
                    .map_err(|e| AppError::InternalError(format!("Header parse error: {}", e)))?,
            );
            headers.insert(
                "content-disposition",
                format!("attachment; filename=\"{filename}\"")
                    .parse()
                    .map_err(|e| AppError::InternalError(format!("Header parse error: {}", e)))?,
            );

            Ok((headers, content))
        }
        None => Err(AppError::NotFound),
    }
}

async fn handle_repo(
    Path((owner, repo)): Path<(String, String)>,
    Query(params): Query<QueryParams>,
) -> Result<impl IntoResponse, AppError> {
    ingest_github_repo(owner, repo, None, None, params).await
}

async fn handle_repo_branch(
    Path((owner, repo, branch)): Path<(String, String, String)>,
    Query(params): Query<QueryParams>,
) -> Result<impl IntoResponse, AppError> {
    ingest_github_repo(owner, repo, Some(branch), None, params).await
}

async fn handle_repo_path(
    Path((owner, repo, branch, path)): Path<(String, String, String, String)>,
    Query(params): Query<QueryParams>,
) -> Result<impl IntoResponse, AppError> {
    ingest_github_repo(owner, repo, Some(branch), Some(path), params).await
}

async fn handle_repo_compare(
    Path((owner, repo, compare_spec)): Path<(String, String, String)>,
    Query(params): Query<QueryParams>,
) -> Result<impl IntoResponse, AppError> {
    if !validate_github_name(&owner) || !validate_github_name(&repo) {
        return Err(AppError::InvalidRequest(
            "Invalid owner or repo name".to_string(),
        ));
    }

    // Parse compare spec (e.g., "main...feature" or "main..feature")
    let (base, head) = parse_compare_spec(&compare_spec).ok_or_else(|| {
        AppError::InvalidRequest(
            "Invalid compare format. Use 'base...head' or 'base..head'".to_string(),
        )
    })?;

    let url = format!("https://github.com/{owner}/{repo}");

    // Generate the diff
    let diff_content = timeout(INGEST_TIMEOUT, async {
        IngestionService::generate_diff(
            &url,
            &base,
            &head,
            params.include.as_deref(),
            params.exclude.as_deref(),
        )
        .await
    })
    .await
    .map_err(|_| AppError::Timeout)?
    .map_err(|e| AppError::InternalError(format!("Failed to generate diff: {}", e)))?;

    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        "text/plain; charset=utf-8"
            .parse()
            .map_err(|e| AppError::InternalError(format!("Header parse error: {}", e)))?,
    );

    Ok((headers, diff_content))
}

fn parse_compare_spec(spec: &str) -> Option<(String, String)> {
    // Support both three-dot (...) and two-dot (..) syntax
    if let Some((base, head)) = spec.split_once("...") {
        if !base.is_empty() && !head.is_empty() {
            Some((base.to_string(), head.to_string()))
        } else {
            None
        }
    } else if let Some((base, head)) = spec.split_once("..") {
        if !base.is_empty() && !head.is_empty() {
            Some((base.to_string(), head.to_string()))
        } else {
            None
        }
    } else {
        None
    }
}

async fn ingest_github_repo(
    owner: String,
    repo: String,
    branch: Option<String>,
    path_prefix: Option<String>,
    params: QueryParams,
) -> Result<impl IntoResponse, AppError> {
    if !validate_github_name(&owner) || !validate_github_name(&repo) {
        return Err(AppError::InvalidRequest(
            "Invalid owner or repo name".to_string(),
        ));
    }

    let url = format!("https://github.com/{owner}/{repo}");

    let ingestion_params = IngestionParams {
        url,
        subpath: params.subpath.clone(),
        branch: branch.or(params.branch),
        path_prefix: path_prefix
            .or(params.path.clone())
            .or(params.subpath.clone())
            .filter(|p| !p.contains("..") && !p.starts_with('/')),
        include_patterns: params
            .include
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        exclude_patterns: params
            .exclude
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        max_file_size: params.max_size.unwrap_or(10 * 1024 * 1024),
        filter_preset: params.preset,
        raw: params.raw.unwrap_or(false),
    };

    let result = timeout(INGEST_TIMEOUT, async {
        IngestionService::ingest(ingestion_params).await
    })
    .await
    .map_err(|_| AppError::Timeout)?
    .map_err(|e| AppError::InternalError(format!("Ingestion failed: {}", e)))?;

    Ok(result.content)
}

pub fn create_router() -> Router {
    let state = AppState::new();

    let router = Router::new()
        .route("/", get(api_info))
        .route("/health", get(health))
        .route("/api/ingest", post(ingest_repository))
        .route("/api/result/{id}", get(get_result))
        .route("/api/download/{id}", get(download_content))
        .route("/{owner}/{repo}", get(handle_repo))
        .route(
            "/{owner}/{repo}/compare/{compare_spec}",
            get(handle_repo_compare),
        )
        .route("/{owner}/{repo}/tree/{branch}", get(handle_repo_branch))
        .route(
            "/{owner}/{repo}/tree/{branch}/{*path}",
            get(handle_repo_path),
        )
        .with_state(state);

    router.layer(
        ServiceBuilder::new()
            .layer(SetResponseHeaderLayer::overriding(
                axum::http::header::X_FRAME_OPTIONS,
                axum::http::HeaderValue::from_static("DENY"),
            ))
            .layer(SetResponseHeaderLayer::overriding(
                axum::http::header::X_CONTENT_TYPE_OPTIONS,
                axum::http::HeaderValue::from_static("nosniff"),
            ))
            .layer(CorsLayer::permissive())
            .layer(CompressionLayer::new()),
    )
}

pub async fn serve(addr: std::net::SocketAddr) -> anyhow::Result<()> {
    let app = create_router();
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("HTTP server listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
