use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use githem_core::{IngestOptions, Ingester, is_remote_url};
use serde::{Deserialize, Serialize};
use tokio::time::timeout;
use tower::ServiceBuilder;
use std::io::IsTerminal;
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    set_header::SetResponseHeaderLayer,
};

const INGEST_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

fn validate_github_name(name: &str) -> bool {
    !name.is_empty() 
    && name.len() <= 39
    && name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
    && !name.starts_with(['-', '.'])
    && !name.ends_with(['-', '.'])
}

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
    #[serde(default)]
    pub include_patterns: Vec<String>,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    #[serde(default = "default_max_file_size")]
    pub max_file_size: usize,
}

fn default_max_file_size() -> usize {
    10 * 1024 * 1024 // 10MB
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionResult {
    pub id: String,
    pub summary: IngestionSummary,
    pub tree: String,
    pub content: String,
    pub metadata: RepositoryMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionSummary {
    pub repository: String,
    pub branch: String,
    pub subpath: Option<String>,
    pub files_analyzed: usize,
    pub total_size: usize,
    pub estimated_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryMetadata {
    pub url: String,
    pub default_branch: String,
    pub branches: Vec<String>,
    pub size: Option<u64>,
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
    IngestionFailed(String),
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
            AppError::IngestionFailed(msg) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                ErrorResponse {
                    error: msg,
                    code: "INGESTION_FAILED".to_string(),
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
}

async fn health() -> impl IntoResponse {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
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
    let start = Instant::now();
    
    // Validate request
    if request.url.is_empty() {
        return Err(AppError::InvalidRequest("URL is required".to_string()));
    }

    if !is_remote_url(&request.url) && !std::path::Path::new(&request.url).exists() {
        return Err(AppError::InvalidRequest("Invalid URL or path".to_string()));
    }

    // Generate unique ID for this ingestion
    let id = format!("{}-{}", 
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis(),
        rand::random::<u32>()
    );

    // Create ingestion options
    let options = IngestOptions {
        include_patterns: request.include_patterns.clone(),
        exclude_patterns: request.exclude_patterns.clone(),
        max_file_size: request.max_file_size,
        include_untracked: false,
        branch: request.branch.clone(),
    };

    // Perform ingestion with timeout
    let ingestion_result = timeout(INGEST_TIMEOUT, async {
        perform_ingestion(request, options, id.clone()).await
    })
    .await
    .map_err(|_| AppError::Timeout)?
    .map_err(|e| AppError::IngestionFailed(e.to_string()))?;

    // Cache the result
    let cached = CachedResult {
        result: ingestion_result.clone(),
        created_at: start,
    };
    
    state.cache.write().await.insert(id.clone(), cached);

    // Clean up old cache entries (keep last 100)
    let mut cache = state.cache.write().await;
    if cache.len() > 100 {
        let mut entries: Vec<_> = cache.iter().map(|(k, v)| (k.clone(), v.created_at)).collect();
        entries.sort_by_key(|(_, time)| *time);
        for (key, _) in entries.iter().take(cache.len() - 100) {
            cache.remove(key);
        }
    }
    drop(cache);

    Ok(Json(IngestResponse {
        id: id.clone(),
        status: "completed".to_string(),
    }))
}

async fn perform_ingestion(
    request: IngestRequest,
    options: IngestOptions,
    id: String,
) -> Result<IngestionResult, Box<dyn std::error::Error + Send + Sync>> {
    // Create ingester
    let ingester = if is_remote_url(&request.url) {
        Ingester::from_url(&request.url, options)?
    } else {
        let path = std::path::PathBuf::from(&request.url);
        Ingester::from_path(&path, options)?
    };

    // Generate content
    let mut content = Vec::new();
    ingester.ingest(&mut content)?;
    let content_str = String::from_utf8(content)?;

    // Generate tree representation (simplified)
    let tree = generate_tree_representation(&content_str);

    // Calculate summary
    let files_analyzed = content_str.matches("=== ").count();
    let total_size = content_str.len();
    let estimated_tokens = estimate_tokens(&content_str);

    let summary = IngestionSummary {
        repository: request.url.clone(),
        branch: request.branch.unwrap_or_else(|| "main".to_string()),
        subpath: request.subpath,
        files_analyzed,
        total_size,
        estimated_tokens,
    };

    // Mock metadata (in real implementation, you'd fetch this from git)
    let metadata = RepositoryMetadata {
        url: request.url,
        default_branch: "main".to_string(),
        branches: vec!["main".to_string(), "develop".to_string()],
        size: Some(total_size as u64),
    };

    Ok(IngestionResult {
        id,
        summary,
        tree,
        content: content_str,
        metadata,
    })
}

fn generate_tree_representation(content: &str) -> String {
    let mut tree = String::new();
    tree.push_str("Repository structure:\n");
    
    for line in content.lines() {
        if line.starts_with("=== ") && line.ends_with(" ===") {
            let path = &line[4..line.len()-4];
            tree.push_str(&format!("📄 {}\n", path));
        }
    }
    
    tree
}

fn estimate_tokens(content: &str) -> usize {
    // Rough estimation: ~4 characters per token
    content.len() / 4
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
        },
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
            let filename = format!("githem-{}.txt", id);
            drop(cache);
            
            let mut headers = HeaderMap::new();
            headers.insert("content-type", "text/plain; charset=utf-8".parse().unwrap());
            headers.insert("content-disposition", format!("attachment; filename=\"{}\"", filename).parse().unwrap());
            
            Ok((headers, content))
        },
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

async fn ingest_github_repo(
    owner: String,
    repo: String,
    branch: Option<String>,
    subpath: Option<String>,
    params: QueryParams,
) -> Result<impl IntoResponse, AppError> {
    // Validate input
    if !validate_github_name(&owner) || !validate_github_name(&repo) {
        return Err(AppError::InvalidRequest("Invalid owner or repo name".to_string()));
    }

    let url = format!("https://github.com/{}/{}", owner, repo);
    let final_branch = branch.or(params.branch);
    let final_subpath = subpath.or(params.subpath);

    let request = IngestRequest {
        url,
        branch: final_branch.clone(),
        subpath: final_subpath,
        include_patterns: params.include
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        exclude_patterns: params.exclude
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        max_file_size: params.max_size.unwrap_or(10 * 1024 * 1024),
    };

    // Actually perform ingestion
    let id = format!("{}-{}", 
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis(),
        rand::random::<u32>()
    );

    let options = IngestOptions {
        include_patterns: request.include_patterns.clone(),
        exclude_patterns: request.exclude_patterns.clone(),
        max_file_size: request.max_file_size,
        include_untracked: false,
        branch: request.branch.clone(),
    };

    let ingestion_result = timeout(INGEST_TIMEOUT, async {
        perform_ingestion(request, options, id.clone()).await
    })
    .await
    .map_err(|_| AppError::Timeout)?
    .map_err(|e| AppError::IngestionFailed(e.to_string()))?;

    // Return the content directly for GitHub-style requests
    Ok(ingestion_result.content)
}

pub fn create_router() -> Router {
    let state = AppState::new();

    let router = Router::new()
        // API routes first - specific paths
        .route("/health", get(health))
        .route("/api/ingest", post(ingest_repository))
        .route("/api/result/{id}", get(get_result))
        .route("/api/download/{id}", get(download_content))

        // GitHub-like routes - exact structure match
        .route("/{owner}/{repo}", get(handle_repo))
        .route("/{owner}/{repo}/tree/{branch}", get(handle_repo_branch))
        .route("/{owner}/{repo}/tree/{branch}/{*path}", get(handle_repo_path))
        .with_state(state);

    // Optional rate limiting
    #[cfg(feature = "rate-limit")]
    if std::env::var("ENABLE_RATE_LIMIT").is_ok() {
        let governor_conf = Arc::new(
            GovernorConfigBuilder::default()
                .key_extractor(SmartIpKeyExtractor)
                .burst_size(10)
                .per_second(1)
                .finish()
                .unwrap()
        );
        
        router = router.layer(GovernorLayer {
            config: governor_conf,
        });
    }

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
            .layer(CompressionLayer::new())
    )
}

// Add serve function for main.rs
pub async fn serve(addr: std::net::SocketAddr) -> anyhow::Result<()> {
    let app = create_router();
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("HTTP server listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}
