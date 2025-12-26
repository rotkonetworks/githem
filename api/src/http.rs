use crate::cache::{CacheStatus, DiffCache, RepositoryCache};
use crate::ingestion::{IngestionParams, IngestionService};
use crate::metrics::MetricsCollector;
use githem_core::validate_github_name;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Json, Response},
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
    pub repo_cache: Arc<RepositoryCache>,
    pub diff_cache: Arc<DiffCache>,
    pub metrics: Arc<MetricsCollector>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        let metrics = Arc::new(MetricsCollector::new());
        Self {
            repo_cache: Arc::new(RepositoryCache::new(
                5 * 1024 * 1024 * 1024,    // 5GB
                Duration::from_secs(3600), // 1 hour TTL
                metrics.clone(),
            )),
            diff_cache: Arc::new(DiffCache::new(10000)), // 10k diff entries
            metrics,
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
    pub filter_preset: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
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
                    hint: Some("check the url format: /{owner}/{repo} or /{owner}/{repo}/tree/{branch}".to_string()),
                    docs: Some("https://githem.com/help.html".to_string()),
                },
            ),
            AppError::NotFound => (
                StatusCode::NOT_FOUND,
                ErrorResponse {
                    error: "resource not found".to_string(),
                    code: "NOT_FOUND".to_string(),
                    hint: Some("valid formats: /{owner}/{repo}, /{owner}/{repo}/tree/{branch}, /{owner}/{repo}/commit/{sha}".to_string()),
                    docs: Some("https://githem.com/help.html".to_string()),
                },
            ),
            AppError::Timeout => (
                StatusCode::REQUEST_TIMEOUT,
                ErrorResponse {
                    error: "request timed out - repository may be too large".to_string(),
                    code: "TIMEOUT".to_string(),
                    hint: Some("try using ?include=src/ to limit scope, or ?preset=code-only".to_string()),
                    docs: Some("https://githem.com/help.html".to_string()),
                },
            ),
            AppError::InternalError(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorResponse {
                    error: msg,
                    code: "INTERNAL_ERROR".to_string(),
                    hint: None,
                    docs: Some("https://github.com/rotkonetworks/githem/issues".to_string()),
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
    /// diff context lines (like git diff -U), defaults to 3
    pub ctx: Option<u32>,
}

// Serve static files
async fn serve_static_file(filename: &str) -> Response {
    let (content, content_type) = match filename {
        "index.html" | "" => (
            include_str!("../../get/web/index.html"),
            "text/html; charset=utf-8",
        ),
        "help.html" => (
            include_str!("../../get/web/help.html"),
            "text/html; charset=utf-8",
        ),
        "styles.css" => (
            include_str!("../../get/web/styles.css"),
            "text/css; charset=utf-8",
        ),
        "globals.css" => (
            include_str!("../../get/web/globals.css"),
            "text/css; charset=utf-8",
        ),
        "install.sh" => (
            include_str!("../../get/install.sh"),
            "text/plain; charset=utf-8",
        ),
        "install.ps1" => (
            include_str!("../../get/install/install.ps1"),
            "text/plain; charset=utf-8",
        ),
        _ => {
            return (StatusCode::NOT_FOUND, Html("404 Not Found")).into_response();
        }
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(axum::body::Body::from(content))
        .unwrap()
}

async fn landing_page() -> Response {
    serve_static_file("index.html").await
}

async fn help_page() -> Response {
    serve_static_file("help.html").await
}

async fn styles_css() -> Response {
    serve_static_file("styles.css").await
}

async fn globals_css() -> Response {
    serve_static_file("globals.css").await
}

async fn install_sh() -> Response {
    serve_static_file("install.sh").await
}

async fn install_ps1() -> Response {
    serve_static_file("install.ps1").await
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let repo_cache_stats = state.repo_cache.stats().await;
    let diff_cache_stats = state.diff_cache.stats().await;

    Json(serde_json::json!({
        "status": "ok",
        "timestamp": timestamp,
        "version": env!("CARGO_PKG_VERSION"),
        "repo_cache": {
            "entries": repo_cache_stats.entries,
            "size_mb": repo_cache_stats.total_size / 1024 / 1024,
            "hit_rate": format!("{:.1}%", repo_cache_stats.hit_rate * 100.0)
        },
        "diff_cache": {
            "entries": diff_cache_stats.entries,
            "size_kb": diff_cache_stats.total_size / 1024
        }
    }))
}

async fn api_info() -> impl IntoResponse {
    Json(serde_json::json!({
        "name": "githem",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "convert git repositories to llm-ready text",
        "endpoints": {
            "repository": "/{owner}/{repo}",
            "branch": "/{owner}/{repo}/tree/{branch}",
            "path": "/{owner}/{repo}/tree/{branch}/{path}",
            "commit": "/{owner}/{repo}/commit/{sha}",
            "compare": "/{owner}/{repo}/compare/{base}...{head}",
            "pull_request": "/{owner}/{repo}/pull/{number}"
        },
        "query_params": {
            "preset": ["raw", "standard", "code-only", "minimal"],
            "include": "comma-separated patterns (e.g. src/,lib/)",
            "exclude": "comma-separated patterns (e.g. tests/,*.md)",
            "branch": "branch name (alternative to /tree/{branch})"
        },
        "examples": [
            "https://githem.com/owner/repo",
            "https://githem.com/owner/repo?preset=code-only",
            "https://githem.com/owner/repo/tree/main/src",
            "https://githem.com/owner/repo/commit/abc123"
        ],
        "docs": "https://githem.com/help.html",
        "source": "https://github.com/rotkonetworks/githem"
    }))
}

#[allow(dead_code)]
async fn version() -> impl IntoResponse {
    Json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "name": env!("CARGO_PKG_NAME"),
        "repository": env!("CARGO_PKG_REPOSITORY"),
        "build_time": option_env!("VERGEN_BUILD_TIMESTAMP").unwrap_or("unknown"),
        "git_commit": option_env!("VERGEN_GIT_SHA").unwrap_or("unknown"),
        "git_branch": option_env!("VERGEN_GIT_BRANCH").unwrap_or("unknown"),
        "rust_version": option_env!("VERGEN_RUSTC_SEMVER").unwrap_or("unknown")
    }))
}

async fn ingest_repository(
    State(state): State<AppState>,
    Json(request): Json<IngestRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.metrics.record_request().await;
    let start = Instant::now();

    // Check cache first
    let cache_key = RepositoryCache::generate_key(
        &request.url,
        request.branch.as_deref(),
        request.filter_preset.as_deref(),
        request.path_prefix.as_deref(),
    );

    if let Some(cached) = state.repo_cache.get(&cache_key).await {
        state.metrics.record_response_time(start.elapsed()).await;
        return Ok(Json(IngestResponse {
            id: cached.result.id.clone(),
            status: "completed".to_string(),
        }));
    }

    let params = IngestionParams {
        url: request.url.clone(),
        subpath: request.subpath.clone(),
        branch: request.branch.clone(),
        path_prefix: request.path_prefix.or(request.subpath),
        include_patterns: request.include_patterns,
        exclude_patterns: request.exclude_patterns,
        max_file_size: request.max_file_size,
        filter_preset: request.filter_preset.clone(),
        raw: request.raw,
    };

    let ingestion_result = match timeout(INGEST_TIMEOUT, async {
        IngestionService::ingest(params).await
    })
    .await
    {
        Ok(Ok(result)) => result,
        Ok(Err(e)) => {
            state.metrics.record_error().await;
            return Err(AppError::InternalError(format!("Ingestion failed: {}", e)));
        }
        Err(_) => {
            state.metrics.record_error().await;
            return Err(AppError::Timeout);
        }
    };

    // Update metrics
    state
        .metrics
        .record_ingestion(
            &request.url,
            ingestion_result.summary.files_analyzed,
            ingestion_result.summary.total_size as u64,
        )
        .await;

    // Get commit hash (simplified - would need actual implementation)
    let commit_hash = ingestion_result.metadata.url.clone();

    // Cache the result
    state
        .repo_cache
        .put(
            cache_key,
            request.url,
            request.branch,
            commit_hash,
            ingestion_result.clone(),
        )
        .await;

    state.metrics.record_response_time(start.elapsed()).await;

    Ok(Json(IngestResponse {
        id: ingestion_result.id.clone(),
        status: "completed".to_string(),
    }))
}

async fn get_result(
    State(state): State<AppState>,
    Path(_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    state.metrics.record_request().await;

    // Check all cache entries for matching ID
    // This is a simplified approach - in production you'd want a separate ID index
    Err::<Json<()>, AppError>(AppError::NotFound)
}

async fn download_content(
    State(state): State<AppState>,
    Path(_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    state.metrics.record_request().await;

    // Similar to get_result but returns as download
    Err::<String, AppError>(AppError::NotFound)
}

async fn handle_repo(
    State(state): State<AppState>,
    Path((owner, repo)): Path<(String, String)>,
    Query(params): Query<QueryParams>,
) -> Result<impl IntoResponse, AppError> {
    ingest_github_repo(state, owner, repo, None, None, params).await
}

async fn handle_repo_branch(
    State(state): State<AppState>,
    Path((owner, repo, branch)): Path<(String, String, String)>,
    Query(params): Query<QueryParams>,
) -> Result<impl IntoResponse, AppError> {
    ingest_github_repo(state, owner, repo, Some(branch), None, params).await
}

async fn handle_repo_path(
    State(state): State<AppState>,
    Path((owner, repo, branch, path)): Path<(String, String, String, String)>,
    Query(params): Query<QueryParams>,
) -> Result<impl IntoResponse, AppError> {
    ingest_github_repo(state, owner, repo, Some(branch), Some(path), params).await
}

async fn handle_pr(
    State(state): State<AppState>,
    Path((owner, repo, pr_number)): Path<(String, String, String)>,
    Query(params): Query<QueryParams>,
) -> Result<impl IntoResponse, AppError> {
    if !validate_github_name(&owner) || !validate_github_name(&repo) {
        return Err(AppError::InvalidRequest(
            "Invalid owner or repo name".to_string(),
        ));
    }

    let pr_num = pr_number.parse::<u32>().map_err(|_| {
        AppError::InvalidRequest("Invalid PR number".to_string())
    })?;

    state.metrics.record_request().await;

    // check cache - PRs can change but cache for a short time
    let context_suffix = params.ctx.map(|c| format!(":ctx{}", c)).unwrap_or_default();
    let cache_key = DiffCache::generate_key("pr", &owner, &repo, &format!("{}{}", pr_number, context_suffix));
    if let Some(cached) = state.diff_cache.get(&cache_key).await {
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-type",
            "text/plain; charset=utf-8".parse().unwrap(),
        );
        return Ok((headers, cached));
    }

    let url = format!("https://github.com/{owner}/{repo}");

    let diff_content = timeout(INGEST_TIMEOUT, async {
        IngestionService::generate_pr_diff(
            &url,
            pr_num,
            params.include.as_deref(),
            params.exclude.as_deref(),
            params.ctx,
        )
        .await
    })
    .await
    .map_err(|_| AppError::Timeout)?
    .map_err(|e| AppError::InternalError(format!("Failed to generate PR diff: {}", e)))?;

    state.diff_cache.put(cache_key, diff_content.clone()).await;

    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        "text/plain; charset=utf-8"
            .parse()
            .map_err(|e| AppError::InternalError(format!("Header parse error: {}", e)))?,
    );

    Ok((headers, diff_content))
}

async fn handle_repo_tag(
    State(state): State<AppState>,
    Path((owner, repo, tag)): Path<(String, String, String)>,
    Query(params): Query<QueryParams>,
) -> Result<impl IntoResponse, AppError> {
    // tag works just like a branch
    ingest_github_repo(state, owner, repo, Some(tag), None, params).await
}

async fn handle_mr(
    State(state): State<AppState>,
    Path((owner, repo, mr_number)): Path<(String, String, String)>,
    Query(params): Query<QueryParams>,
) -> Result<impl IntoResponse, AppError> {
    if !validate_github_name(&owner) || !validate_github_name(&repo) {
        return Err(AppError::InvalidRequest(
            "Invalid owner or repo name".to_string(),
        ));
    }

    let mr_num = mr_number.parse::<u32>().map_err(|_| {
        AppError::InvalidRequest("Invalid MR number".to_string())
    })?;

    state.metrics.record_request().await;

    // check cache
    let context_suffix = params.ctx.map(|c| format!(":ctx{}", c)).unwrap_or_default();
    let cache_key = DiffCache::generate_key("mr", &owner, &repo, &format!("{}{}", mr_number, context_suffix));
    if let Some(cached) = state.diff_cache.get(&cache_key).await {
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-type",
            "text/plain; charset=utf-8".parse().unwrap(),
        );
        return Ok((headers, cached));
    }

    let url = format!("https://gitlab.com/{owner}/{repo}");

    let diff_content = timeout(INGEST_TIMEOUT, async {
        IngestionService::generate_mr_diff(
            &url,
            mr_num,
            params.include.as_deref(),
            params.exclude.as_deref(),
            params.ctx,
        )
        .await
    })
    .await
    .map_err(|_| AppError::Timeout)?
    .map_err(|e| AppError::InternalError(format!("Failed to generate MR diff: {}", e)))?;

    state.diff_cache.put(cache_key, diff_content.clone()).await;

    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        "text/plain; charset=utf-8"
            .parse()
            .map_err(|e| AppError::InternalError(format!("Header parse error: {}", e)))?,
    );

    Ok((headers, diff_content))
}

async fn handle_commit(
    State(state): State<AppState>,
    Path((owner, repo, commit_sha)): Path<(String, String, String)>,
    Query(params): Query<QueryParams>,
) -> Result<impl IntoResponse, AppError> {
    if !validate_github_name(&owner) || !validate_github_name(&repo) {
        return Err(AppError::InvalidRequest(
            "Invalid owner or repo name".to_string(),
        ));
    }

    // validate commit sha format (7-40 hex chars)
    if commit_sha.len() < 7 || commit_sha.len() > 40 || !commit_sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::InvalidRequest(
            "Invalid commit SHA format".to_string(),
        ));
    }

    state.metrics.record_request().await;

    // check cache first - commits are immutable, but context param matters
    let context_suffix = params.ctx.map(|c| format!(":ctx{}", c)).unwrap_or_default();
    let cache_key = DiffCache::generate_key("commit", &owner, &repo, &format!("{}{}", commit_sha, context_suffix));
    if let Some(cached) = state.diff_cache.get(&cache_key).await {
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-type",
            "text/plain; charset=utf-8".parse().unwrap(),
        );
        return Ok((headers, cached));
    }

    let url = format!("https://github.com/{owner}/{repo}");

    let diff_content = timeout(INGEST_TIMEOUT, async {
        IngestionService::generate_commit_diff(
            &url,
            &commit_sha,
            params.include.as_deref(),
            params.exclude.as_deref(),
            params.ctx,
        )
        .await
    })
    .await
    .map_err(|_| AppError::Timeout)?
    .map_err(|e| AppError::InternalError(format!("Failed to generate commit diff: {}", e)))?;

    // cache the result
    state.diff_cache.put(cache_key, diff_content.clone()).await;

    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        "text/plain; charset=utf-8"
            .parse()
            .map_err(|e| AppError::InternalError(format!("Header parse error: {}", e)))?,
    );

    Ok((headers, diff_content))
}

async fn handle_repo_compare(
    State(state): State<AppState>,
    Path((owner, repo, compare_spec)): Path<(String, String, String)>,
    Query(params): Query<QueryParams>,
) -> Result<impl IntoResponse, AppError> {
    if !validate_github_name(&owner) || !validate_github_name(&repo) {
        return Err(AppError::InvalidRequest(
            "Invalid owner or repo name".to_string(),
        ));
    }

    let (base, head) = parse_compare_spec(&compare_spec).ok_or_else(|| {
        AppError::InvalidRequest(
            "Invalid compare format. Use 'base...head' or 'base..head'".to_string(),
        )
    })?;

    state.metrics.record_request().await;

    // check cache
    let context_suffix = params.ctx.map(|c| format!(":ctx{}", c)).unwrap_or_default();
    let cache_key = DiffCache::generate_key("compare", &owner, &repo, &format!("{}{}", compare_spec, context_suffix));
    if let Some(cached) = state.diff_cache.get(&cache_key).await {
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-type",
            "text/plain; charset=utf-8".parse().unwrap(),
        );
        return Ok((headers, cached));
    }

    let url = format!("https://github.com/{owner}/{repo}");

    let diff_content = timeout(INGEST_TIMEOUT, async {
        IngestionService::generate_diff(
            &url,
            &base,
            &head,
            params.include.as_deref(),
            params.exclude.as_deref(),
            params.ctx,
        )
        .await
    })
    .await
    .map_err(|_| AppError::Timeout)?
    .map_err(|e| AppError::InternalError(format!("Failed to generate diff: {}", e)))?;

    state.diff_cache.put(cache_key, diff_content.clone()).await;

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
    state: AppState,
    owner: String,
    repo: String,
    branch: Option<String>,
    path_prefix: Option<String>,
    params: QueryParams,
) -> Result<impl IntoResponse, AppError> {
    state.metrics.record_request().await;
    let start = Instant::now();

    if !validate_github_name(&owner) || !validate_github_name(&repo) {
        state.metrics.record_error().await;
        return Err(AppError::InvalidRequest(
            "Invalid owner or repo name".to_string(),
        ));
    }

    let url = format!("https://github.com/{owner}/{repo}");
    let effective_branch = branch.clone().or(params.branch.clone());

    // Check cache with smart validation
    let cache_key = RepositoryCache::generate_key(
        &url,
        effective_branch.as_deref(),
        params.preset.as_deref(),
        path_prefix
            .as_ref()
            .or(params.path.as_ref())
            .or(params.subpath.as_ref())
            .map(|s| s.as_str()),
    );

    let (cache_status, cached_commit) = state.repo_cache.check_status(&cache_key).await;

    match cache_status {
        CacheStatus::Fresh => {
            // < 5 min old, serve immediately
            if let Some(cached) = state.repo_cache.get(&cache_key).await {
                state.metrics.record_response_time(start.elapsed()).await;
                return Ok(cached.result.content);
            }
        }
        CacheStatus::Valid => {
            // 5min-24h old, validate commit hash
            if let Some(cached_hash) = cached_commit {
                // quick ls-remote check
                if let Ok(current_hash) = githem_core::get_remote_head(&url, effective_branch.as_deref()) {
                    if current_hash == cached_hash {
                        // commit unchanged, serve cached and update validation time
                        state.repo_cache.mark_validated(&cache_key).await;
                        if let Some(cached) = state.repo_cache.get(&cache_key).await {
                            state.metrics.record_response_time(start.elapsed()).await;
                            return Ok(cached.result.content);
                        }
                    } else {
                        // commit changed, invalidate cache
                        state.repo_cache.invalidate(&cache_key).await;
                    }
                }
                // if ls-remote fails, fall through to full fetch
            }
        }
        CacheStatus::Expired | CacheStatus::Stale | CacheStatus::Miss => {
            // need fresh fetch
        }
    }

    let ingestion_params = IngestionParams {
        url: url.clone(),
        subpath: params.subpath.clone(),
        branch: branch.clone().or(params.branch.clone()),
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
        filter_preset: params.preset.clone(),
        raw: params.raw.unwrap_or(false),
    };

    let result = match timeout(INGEST_TIMEOUT, async {
        IngestionService::ingest(ingestion_params).await
    })
    .await
    {
        Ok(Ok(result)) => result,
        Ok(Err(e)) => {
            state.metrics.record_error().await;
            return Err(AppError::InternalError(format!("Ingestion failed: {}", e)));
        }
        Err(_) => {
            state.metrics.record_error().await;
            return Err(AppError::Timeout);
        }
    };

    // Update metrics
    state
        .metrics
        .record_ingestion(
            &url,
            result.summary.files_analyzed,
            result.summary.total_size as u64,
        )
        .await;

    // Cache the result with commit hash
    // TODO: get actual commit hash from ingestion result
    let commit_hash = githem_core::get_remote_head(&url, effective_branch.as_deref())
        .unwrap_or_else(|_| result.metadata.url.clone());
    state
        .repo_cache
        .put(cache_key, url, effective_branch, commit_hash, result.clone())
        .await;

    state.metrics.record_response_time(start.elapsed()).await;

    Ok(result.content)
}

async fn get_top_repos(State(state): State<AppState>) -> impl IntoResponse {
    let repos = state.metrics.get_top_repositories(10).await;
    Json(repos)
}

async fn get_metrics(State(state): State<AppState>) -> impl IntoResponse {
    let metrics = state.metrics.get_metrics().await;
    Json(metrics)
}

async fn get_cache_stats(State(state): State<AppState>) -> impl IntoResponse {
    let stats = state.repo_cache.stats().await;
    Json(stats)
}

pub fn create_router() -> Router {
    let state = AppState::new();

    let router = Router::new()
        // Landing page and static assets
        .route("/", get(landing_page))
        .route("/help.html", get(help_page))
        .route("/styles.css", get(styles_css))
        .route("/globals.css", get(globals_css))
        .route("/install.sh", get(install_sh))
        .route("/install.ps1", get(install_ps1))
        // API endpoints
        .route("/api", get(api_info))
        .route("/health", get(health))
        .route("/metrics", get(get_metrics))
        .route("/api/metrics/top", get(get_top_repos))
        .route("/cache/stats", get(get_cache_stats))
        .route("/api/ingest", post(ingest_repository))
        .route("/api/result/{id}", get(get_result))
        .route("/api/download/{id}", get(download_content))
        // GitHub repository routes
        .route("/{owner}/{repo}", get(handle_repo))
        .route("/{owner}/{repo}/pull/{pr_number}", get(handle_pr))
        .route("/{owner}/{repo}/commit/{commit_sha}", get(handle_commit))
        .route(
            "/{owner}/{repo}/compare/{compare_spec}",
            get(handle_repo_compare),
        )
        .route("/{owner}/{repo}/tree/{branch}", get(handle_repo_branch))
        .route(
            "/{owner}/{repo}/tree/{branch}/{*path}",
            get(handle_repo_path),
        )
        // blob routes (same as tree, just different github url pattern)
        .route("/{owner}/{repo}/blob/{branch}", get(handle_repo_branch))
        .route(
            "/{owner}/{repo}/blob/{branch}/{*path}",
            get(handle_repo_path),
        )
        // releases/tags
        .route(
            "/{owner}/{repo}/releases/tag/{tag}",
            get(handle_repo_tag),
        )
        // gitlab routes (uses /-/ separator)
        .route("/{owner}/{repo}/-/tree/{branch}", get(handle_repo_branch))
        .route(
            "/{owner}/{repo}/-/tree/{branch}/{*path}",
            get(handle_repo_path),
        )
        .route("/{owner}/{repo}/-/blob/{branch}", get(handle_repo_branch))
        .route(
            "/{owner}/{repo}/-/blob/{branch}/{*path}",
            get(handle_repo_path),
        )
        .route(
            "/{owner}/{repo}/-/commit/{commit_sha}",
            get(handle_commit),
        )
        .route(
            "/{owner}/{repo}/-/compare/{compare_spec}",
            get(handle_repo_compare),
        )
        .route(
            "/{owner}/{repo}/-/merge_requests/{mr_number}",
            get(handle_mr),
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
