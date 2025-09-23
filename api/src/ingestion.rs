use githem_core::{
    count_files, estimate_tokens, generate_tree, is_remote_url, normalize_source_url, FilterPreset,
    FilterStats, IngestOptions, Ingester, IngestionCallback,
};

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionParams {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionResult {
    pub id: String,
    pub summary: IngestionSummary,
    pub tree: String,
    pub content: String,
    pub metadata: RepositoryMetadata,
    pub filter_stats: Option<FilterStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionSummary {
    pub repository: String,
    pub branch: String,
    pub subpath: Option<String>,
    pub files_analyzed: usize,
    pub total_size: usize,
    pub estimated_tokens: usize,
    pub filter_preset: String,
    pub filtering_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryMetadata {
    pub url: String,
    pub default_branch: String,
    pub branches: Vec<String>,
    pub size: Option<u64>,
}

pub struct IngestionService;

impl IngestionService {
    pub async fn ingest(
        params: IngestionParams,
    ) -> Result<IngestionResult, Box<dyn std::error::Error + Send + Sync>> {
        let params = Self::normalize_params(params)?;

        let filter_preset = if params.raw {
            Some(FilterPreset::Raw)
        } else if let Some(preset) = Self::parse_filter_preset(params.filter_preset.as_deref()) {
            Some(preset)
        } else {
            Some(FilterPreset::Standard)
        };

        let filter_preset_name = match filter_preset {
            Some(FilterPreset::Raw) => "raw",
            Some(FilterPreset::Standard) => "standard",
            Some(FilterPreset::CodeOnly) => "code-only",
            Some(FilterPreset::Minimal) => "minimal",
            None => "none",
        };

        let options = IngestOptions {
            include_patterns: params.include_patterns.clone(),
            exclude_patterns: params.exclude_patterns.clone(),
            max_file_size: params.max_file_size,
            include_untracked: false,
            branch: params.branch.clone(),
            path_prefix: params.path_prefix.clone(),
            filter_preset,
            apply_default_filters: false,
        };

        let mut ingester = if is_remote_url(&params.url) {
            Ingester::from_url_cached(&params.url, options)?
        } else {
            let path = std::path::PathBuf::from(&params.url);
            Ingester::from_path(&path, options)?
        };

        let filter_stats = ingester.get_filter_stats().ok();

        let mut content = Vec::new();
        if ingester.cache_key.is_some() {
            ingester.ingest_cached(&mut content)?;
        } else {
            ingester.ingest(&mut content)?;
        }

        let content_str = String::from_utf8(content)?;

        let id = format!(
            "{}-{}",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
            rand::random::<u32>()
        );

        let tree = generate_tree(&content_str);
        let files_analyzed = count_files(&content_str);
        let total_size = content_str.len();
        let estimated_tokens = estimate_tokens(&content_str);

        let summary = IngestionSummary {
            repository: params.url.clone(),
            branch: params.branch.unwrap_or_else(|| "main".to_string()),
            subpath: params.path_prefix.clone(),
            files_analyzed,
            total_size,
            estimated_tokens,
            filter_preset: filter_preset_name.to_string(),
            filtering_enabled: filter_preset != Some(FilterPreset::Raw),
        };

        let metadata = RepositoryMetadata {
            url: params.url,
            default_branch: "main".to_string(),
            branches: vec!["main".to_string()],
            size: Some(total_size as u64),
        };

        Ok(IngestionResult {
            id,
            summary,
            tree,
            content: content_str,
            metadata,
            filter_stats,
        })
    }

    pub fn normalize_params(params: IngestionParams) -> Result<IngestionParams, String> {
        if params.url.is_empty() {
            return Err("URL is required".to_string());
        }

        let (normalized_url, final_branch, final_path_prefix) = normalize_source_url(
            &params.url,
            params.branch.clone(),
            params.path_prefix.clone(),
        )?;

        if !is_remote_url(&normalized_url) && !std::path::Path::new(&normalized_url).exists() {
            return Err("Invalid URL or path".to_string());
        }

        Ok(IngestionParams {
            url: normalized_url,
            subpath: params.subpath,
            branch: final_branch,
            path_prefix: final_path_prefix,
            include_patterns: params.include_patterns,
            exclude_patterns: params.exclude_patterns,
            max_file_size: params.max_file_size,
            filter_preset: params.filter_preset,
            raw: params.raw,
        })
    }

    pub fn parse_filter_preset(preset_str: Option<&str>) -> Option<FilterPreset> {
        preset_str.and_then(|s| match s.to_lowercase().as_str() {
            "raw" => Some(FilterPreset::Raw),
            "standard" => Some(FilterPreset::Standard),
            "code-only" | "code_only" | "codeonly" => Some(FilterPreset::CodeOnly),
            "minimal" => Some(FilterPreset::Minimal),
            _ => None,
        })
    }

    pub async fn generate_diff(
        url: &str,
        base: &str,
        head: &str,
        _include_patterns: Option<&str>,
        _exclude_patterns: Option<&str>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let options = IngestOptions::default();
        let ingester = if is_remote_url(url) {
            Ingester::from_url(url, options)?
        } else {
            return Err("Diff generation requires a remote URL".into());
        };

        let diff_content = ingester.generate_diff(base, head)?;
        Ok(diff_content)
    }
}

pub struct WebSocketCallback<F>
where
    F: FnMut(WebSocketMessage),
{
    pub send_fn: F,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum WebSocketMessage {
    Progress { stage: String, message: String },
    File { path: String, content: String },
    Complete { files: usize, bytes: usize },
    Error { message: String },
    FilterStats { stats: FilterStats },
}

impl<F> IngestionCallback for WebSocketCallback<F>
where
    F: FnMut(WebSocketMessage) + Send + Sync,
{
    fn on_progress(&mut self, stage: &str, message: &str) {
        (self.send_fn)(WebSocketMessage::Progress {
            stage: stage.to_string(),
            message: message.to_string(),
        });
    }

    fn on_file(&mut self, path: &Path, content: &str) {
        (self.send_fn)(WebSocketMessage::File {
            path: path.display().to_string(),
            content: content.to_string(),
        });
    }

    fn on_complete(&mut self, files: usize, bytes: usize) {
        (self.send_fn)(WebSocketMessage::Complete { files, bytes });
    }

    fn on_error(&mut self, error: &str) {
        (self.send_fn)(WebSocketMessage::Error {
            message: error.to_string(),
        });
    }
}
